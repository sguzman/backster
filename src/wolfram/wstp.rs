use std::ffi::{c_char, c_int, c_uchar, c_void, CStr, CString};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

#[allow(non_camel_case_types)]
type WSENV = *mut c_void;
#[allow(non_camel_case_types)]
type WSLINK = *mut c_void;

unsafe extern "C" {
    fn WSInitialize(param: *mut c_void) -> WSENV;
    fn WSDeinitialize(env: WSENV);

    fn WSOpenString(env: WSENV, command_line: *const c_char, error: *mut c_int) -> WSLINK;
    fn WSActivate(link: WSLINK) -> c_int;
    fn WSClose(link: WSLINK);

    fn WSError(link: WSLINK) -> c_int;
    fn WSErrorMessage(env: WSENV, error: c_int) -> *const c_char;

    fn WSReady(link: WSLINK) -> c_int;
    fn WSNextPacket(link: WSLINK) -> c_int;
    fn WSNewPacket(link: WSLINK) -> c_int;
    fn WSEndPacket(link: WSLINK) -> c_int;
    fn WSFlush(link: WSLINK) -> c_int;

    fn WSPutFunction(link: WSLINK, f: *const c_char, arg_count: c_int) -> c_int;
    fn WSPutString(link: WSLINK, s: *const c_char) -> c_int;
    fn WSPutSymbol(link: WSLINK, s: *const c_char) -> c_int;

    fn WSGetString(link: WSLINK, s: *mut *const c_char) -> c_int;
    fn WSReleaseString(link: WSLINK, s: *const c_char) -> c_int;
}

const RETURNPKT: c_int = 3;

#[derive(Debug, Clone)]
pub struct WolframSessionConfig {
    /// Kernel executable, e.g. `WolframKernel` or an absolute path.
    pub kernel: String,
}

impl Default for WolframSessionConfig {
    fn default() -> Self {
        Self {
            kernel: std::env::var("WOLFRAMKERNEL").unwrap_or_else(|_| "WolframKernel".to_string()),
        }
    }
}

pub struct WolframSession {
    env: WSENV,
    link: WSLINK,
}

impl WolframSession {
    const DEFAULT_EVAL_TIMEOUT: Duration = Duration::from_secs(90);

    pub fn connect(config: WolframSessionConfig) -> Result<Self> {
        let env = unsafe { WSInitialize(std::ptr::null_mut()) };
        anyhow::ensure!(!env.is_null(), "WSInitialize failed (env is null)");

        let linkname = format!("{} -wstp", config.kernel);
        let cmd = format!("-linkmode launch -linkname '{}'", linkname.replace('\'', "\\'"));
        let cmd_c = CString::new(cmd).context("Invalid WSTP command string")?;

        let mut error: c_int = 0;
        let link = unsafe { WSOpenString(env, cmd_c.as_ptr(), &mut error as *mut c_int) };
        if link.is_null() || error != 0 {
            let msg = unsafe { error_message(env, error) };
            unsafe { WSDeinitialize(env) };
            anyhow::bail!("WSOpenString failed (error={error}): {msg}");
        }
        let ok = unsafe { WSActivate(link) };
        if ok == 0 {
            let err = unsafe { WSError(link) };
            let msg = unsafe { error_message(env, err) };
            unsafe { WSClose(link) };
            unsafe { WSDeinitialize(env) };
            anyhow::bail!("WSActivate failed: WSError={err} ({msg})");
        }

        let mut session = Self { env, link };
        session.drain_startup_packets()?;
        Ok(session)
    }

    /// Evaluates Wolfram Language code and returns `ToString[..., InputForm]`.
    pub fn eval_to_string(&mut self, code: &str) -> Result<String> {
        let code_c = CString::new(code).context("Code contains NUL byte")?;

        // EvaluatePacket[ToString[ToExpression[code], InputForm]]
        self.put_function("EvaluatePacket", 1)?;
        self.put_function("ToString", 2)?;
        self.put_function("ToExpression", 1)?;
        self.put_string(code_c.as_c_str())?;
        self.put_symbol("InputForm")?;
        self.end_packet()?;
        self.flush()?;

        self.read_return_string_with_timeout(Self::DEFAULT_EVAL_TIMEOUT)
            .with_context(|| format!("Failed evaluating WSTP code: {code}"))
    }

    /// Evaluates a Wolfram Language expression that must return a string.
    ///
    /// This avoids `ToString[..., InputForm]` escaping, which is important when
    /// you want raw JSON from `ExportString[..., "JSON"]`.
    pub fn eval_to_string_expr(&mut self, expr: &str) -> Result<String> {
        let expr_c = CString::new(expr).context("Expression contains NUL byte")?;

        // EvaluatePacket[ToExpression[expr]]
        self.put_function("EvaluatePacket", 1)?;
        self.put_function("ToExpression", 1)?;
        self.put_string(expr_c.as_c_str())?;
        self.end_packet()?;
        self.flush()?;

        self.read_return_string_with_timeout(Self::DEFAULT_EVAL_TIMEOUT)
            .with_context(|| format!("Failed evaluating WSTP expr: {expr}"))
    }

    fn put_function(&mut self, name: &str, argc: i32) -> Result<()> {
        let c = CString::new(name).context("Function name contains NUL byte")?;
        let ok = unsafe { WSPutFunction(self.link, c.as_ptr(), argc as c_int) };
        anyhow::ensure!(ok != 0, "WSPutFunction failed: {name}");
        Ok(())
    }

    fn put_string(&mut self, s: &CStr) -> Result<()> {
        let ok = unsafe { WSPutString(self.link, s.as_ptr()) };
        anyhow::ensure!(ok != 0, "WSPutString failed");
        Ok(())
    }

    fn put_symbol(&mut self, s: &str) -> Result<()> {
        let c = CString::new(s).context("Symbol contains NUL byte")?;
        let ok = unsafe { WSPutSymbol(self.link, c.as_ptr()) };
        anyhow::ensure!(ok != 0, "WSPutSymbol failed: {s}");
        Ok(())
    }

    fn end_packet(&mut self) -> Result<()> {
        let ok = unsafe { WSEndPacket(self.link) };
        anyhow::ensure!(ok != 0, "WSEndPacket failed");
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        let ok = unsafe { WSFlush(self.link) };
        anyhow::ensure!(ok != 0, "WSFlush failed");
        Ok(())
    }

    fn read_return_string_with_timeout(&mut self, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;

        loop {
            let Some(pkt) = self.next_packet_until(deadline)? else {
                anyhow::bail!(
                    "Timed out waiting for Wolfram RETURNPKT after {:?}",
                    timeout
                );
            };

            if pkt == RETURNPKT {
                let mut out: *const c_char = std::ptr::null();
                let ok = unsafe { WSGetString(self.link, &mut out as *mut *const c_char) };
                anyhow::ensure!(ok != 0, "WSGetString failed");
                let s = unsafe { CStr::from_ptr(out) }.to_string_lossy().to_string();
                unsafe { WSReleaseString(self.link, out) };
                unsafe { WSNewPacket(self.link) };
                return Ok(s);
            }

            unsafe { WSNewPacket(self.link) };
        }
    }

    fn next_packet_until(&mut self, deadline: Instant) -> Result<Option<c_int>> {
        loop {
            if Instant::now() >= deadline {
                return Ok(None);
            }

            let ready = unsafe { WSReady(self.link) };
            if ready == 0 {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }

            let pkt = unsafe { WSNextPacket(self.link) };
            if pkt == 0 {
                self.fail_with_ws_error("WSNextPacket returned 0")?;
            }
            return Ok(Some(pkt));
        }
    }

    fn drain_startup_packets(&mut self) -> Result<()> {
        // On startup the kernel typically sends prompt/side-effect packets and then
        // waits for input. Calling `WSNextPacket` unconditionally can block forever.
        // Drain only packets that are already available.
        for _ in 0..64 {
            let ready = unsafe { WSReady(self.link) };
            if ready == 0 {
                break;
            }
            let pkt = unsafe { WSNextPacket(self.link) };
            if pkt == 0 {
                self.fail_with_ws_error("Failed draining initial WSTP packets")?;
            }
            unsafe { WSNewPacket(self.link) };
        }
        Ok(())
    }

    fn fail_with_ws_error(&mut self, context: &str) -> Result<()> {
        let err = unsafe { WSError(self.link) };
        let msg = unsafe { error_message(self.env, err) };
        anyhow::bail!("{context}: WSError={err} ({msg})")
    }

    /// Evaluates `Get["/path/to/file.wls"]` (or `.wl`) in the kernel session.
    pub fn load_file(&mut self, path: &std::path::Path) -> Result<()> {
        let path = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Non-UTF8 path not supported for Wolfram load_file"))?;
        let escaped = path.replace('\\', "\\\\").replace('\"', "\\\"");
        let expr = format!("Module[{{}}, Get[\"{escaped}\"]; \"OK\"]");
        let _ = self.eval_to_string_expr(&expr)?;
        Ok(())
    }
}

impl Drop for WolframSession {
    fn drop(&mut self) {
        unsafe {
            if !self.link.is_null() {
                WSClose(self.link);
                self.link = std::ptr::null_mut();
            }
            if !self.env.is_null() {
                WSDeinitialize(self.env);
                self.env = std::ptr::null_mut();
            }
        }
    }
}

unsafe fn error_message(env: WSENV, err: c_int) -> String {
    let ptr = unsafe { WSErrorMessage(env, err) };
    if ptr.is_null() {
        return format!("Unknown WSTP error {err}");
    }
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string()
}

// Keep the file anchored to WSTP presence; this also gives a slightly nicer error
// if the ABI is wrong at runtime.
#[allow(dead_code)]
fn _wstp_abi_sanity() {
    let _ = std::mem::size_of::<WSENV>();
    let _ = std::mem::size_of::<WSLINK>();
    let _ = std::mem::size_of::<c_int>();
    let _ = std::mem::size_of::<c_char>();
    let _ = std::mem::size_of::<c_uchar>();
}
