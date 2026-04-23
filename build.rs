use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let wstp_dir = find_wstp_dir().unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    let (include_dir, lib_dir) = resolve_include_and_lib(&wstp_dir).unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    println!("cargo:rerun-if-env-changed=WSTP_DIR");
    println!("cargo:rerun-if-env-changed=WOLFRAM_DIR");
    println!("cargo:rerun-if-env-changed=WOLFRAMKERNEL");
    println!("cargo:rerun-if-changed=build.rs");

    println!("cargo:rustc-env=WSTP_INCLUDE_DIR={}", include_dir.display());
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        // Ensure the runtime loader can find libWSTP without requiring LD_LIBRARY_PATH.
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    }

    let link_name = pick_wstp_lib(&lib_dir).unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });
    println!("cargo:rustc-link-lib=dylib={link_name}");
}

fn find_wstp_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = env::var("WSTP_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Ok(p);
        }
        return Err(format!(
            "WSTP_DIR is set but does not exist: {}",
            p.display()
        ));
    }

    if let Ok(dir) = env::var("WOLFRAM_DIR") {
        let root = PathBuf::from(dir);
        let guess = root.join("SystemFiles/Links/WSTP/DeveloperKit/Linux-x86-64/CompilerAdditions");
        if guess.exists() {
            return Ok(guess);
        }
        return Err(format!(
            "WOLFRAM_DIR is set but WSTP CompilerAdditions not found at: {}",
            guess.display()
        ));
    }

    if let Some(found) = find_wstp_under("/usr/local/Wolfram/Wolfram") {
        return Ok(found);
    }
    if let Some(found) = find_wstp_under("/opt/Wolfram/Wolfram") {
        return Ok(found);
    }

    Err(
        "WSTP not found.\nSet WSTP_DIR to the WSTP DeveloperKit CompilerAdditions directory (it contains wstp.h and libWSTP*), e.g.:\n  export WSTP_DIR=/usr/local/Wolfram/Wolfram/<VERSION>/SystemFiles/Links/WSTP/DeveloperKit/Linux-x86-64/CompilerAdditions"
            .to_string(),
    )
}

fn find_wstp_under(root: &str) -> Option<PathBuf> {
    let root = Path::new(root);
    let Ok(entries) = fs::read_dir(root) else {
        return None;
    };

    // Prefer highest lexical version directory if multiple.
    let mut versions: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    versions.sort();
    versions.reverse();

    for v in versions {
        let guess = v.join("SystemFiles/Links/WSTP/DeveloperKit/Linux-x86-64/CompilerAdditions");
        if guess.exists() {
            return Some(guess);
        }
    }
    None
}

fn resolve_include_and_lib(wstp_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let include_dir = if wstp_dir.join("wstp.h").exists() {
        wstp_dir.to_path_buf()
    } else if wstp_dir.join("include/wstp.h").exists() {
        wstp_dir.join("include")
    } else {
        return Err(format!(
            "Could not find wstp.h under {} (expected wstp.h or include/wstp.h)",
            wstp_dir.display()
        ));
    };

    let lib_dir = if has_wstp_lib(wstp_dir) {
        wstp_dir.to_path_buf()
    } else if has_wstp_lib(&wstp_dir.join("lib")) {
        wstp_dir.join("lib")
    } else {
        return Err(format!(
            "Could not find libWSTP* under {} (expected libWSTP*.so/.a in this dir or lib/)",
            wstp_dir.display()
        ));
    };

    Ok((include_dir, lib_dir))
}

fn has_wstp_lib(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name();
        let Some(name) = name.to_str() else { return false };
        name.starts_with("libWSTP") && (name.ends_with(".so") || name.ends_with(".a"))
    })
}

fn pick_wstp_lib(lib_dir: &Path) -> Result<String, String> {
    let entries = fs::read_dir(lib_dir)
        .map_err(|e| format!("Failed reading WSTP lib dir {}: {e}", lib_dir.display()))?;

    // Prefer dynamic libs if present.
    let mut candidates: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|n| n.starts_with("libWSTP"))
                && p.extension().and_then(OsStr::to_str).is_some_and(|ext| ext == "so" || ext == "a")
        })
        .collect();

    candidates.sort();
    let preferred = candidates
        .iter()
        .find(|p| p.extension().and_then(OsStr::to_str) == Some("so"))
        .or_else(|| candidates.first())
        .ok_or_else(|| format!("No libWSTP* found under {}", lib_dir.display()))?;

    let file = preferred
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| "Invalid libWSTP filename".to_string())?;

    // Convert libWSTP64i4.so -> WSTP64i4
    let name = file
        .strip_prefix("lib")
        .unwrap_or(file)
        .trim_end_matches(".so")
        .trim_end_matches(".a")
        .to_string();
    Ok(name)
}
