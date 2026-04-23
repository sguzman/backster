use std::cell::RefCell;
use std::rc::{Rc, Weak};

use ahash::AHashMap;
use polars::prelude::*;

use nautilus_common::cache::Cache;
use nautilus_common::clock::{Clock, TestClock};
use nautilus_common::msgbus::{self, MessagingSwitchboard};
use nautilus_common::runner::drain_trading_cmd_queue;
use nautilus_core::{UUID4, UnixNanos};
use nautilus_execution::client::ExecutionClient;
use nautilus_execution::engine::ExecutionEngine;
use nautilus_execution::matching_engine::adapter::OrderEngineAdapter;
use nautilus_execution::models::fee::FeeModelAny;
use nautilus_execution::models::fill::FillModelAny;
use nautilus_model::accounts::{AccountAny, CashAccount};
use nautilus_model::data::{Bar, BarType, BarSpecification};
use nautilus_model::enums::{
    AccountType, AggregationSource, BarAggregation, BookType, OmsType, PriceType,
};
use nautilus_model::events::{AccountState, OrderEventAny};
use nautilus_model::identifiers::{AccountId, InstrumentId, TraderId, Venue};
use nautilus_model::instruments::InstrumentAny;
use nautilus_model::orders::OrderAny;
use nautilus_model::types::{AccountBalance, Currency, Money, Price, Quantity};
use nautilus_portfolio::portfolio::Portfolio;

/// A simulated execution client that bridges the ExecutionEngine to the OrderMatchingEngine.
pub struct SimExecutionClient {
    execution_engine: Weak<RefCell<ExecutionEngine>>,
    matching_engines: Vec<OrderEngineAdapter>,
    trader_id: TraderId,
}

impl SimExecutionClient {
    pub fn new(trader_id: TraderId) -> Self {
        Self {
            execution_engine: Weak::new(),
            matching_engines: Vec::new(),
            trader_id,
        }
    }

    pub fn set_execution_engine(&mut self, execution_engine: Rc<RefCell<ExecutionEngine>>) {
        self.execution_engine = Rc::downgrade(&execution_engine);
    }

    pub fn add_matching_engine(&mut self, adapter: OrderEngineAdapter) {
        // Register an event handler to bridge events back to the ExecutionEngine
        let ee_weak = self.execution_engine.clone();
        adapter.get_engine_mut().set_event_handler(Rc::new(move |event| {
            if let Some(ee) = ee_weak.upgrade() {
                ee.borrow_mut().process(&event);
            }
        }));
        self.matching_engines.push(adapter);
    }

    pub fn process_bar(&self, bar: &Bar) {
        for adapter in &self.matching_engines {
            let mut engine = adapter.get_engine_mut();
            if engine.instrument().id() == bar.bar_type.instrument_id() {
                engine.process_bar(bar);
            }
        }
    }
}

impl ExecutionClient for SimExecutionClient {
    fn trader_id(&self) -> TraderId {
        self.trader_id.clone()
    }

    fn submit_order(&self, order: &OrderAny) -> anyhow::Result<()> {
        let venue = order.instrument_id().venue;
        for adapter in &self.matching_engines {
            let mut engine = adapter.get_engine_mut();
            if engine.venue == venue && engine.instrument().id() == order.instrument_id() {
                engine.process_submit(order, order.account_id());
                return Ok(());
            }
        }
        anyhow::bail!("No matching engine found for venue {}", venue);
    }

    fn cancel_order(&self, order: &OrderAny) -> anyhow::Result<()> {
        let venue = order.instrument_id().venue;
        for adapter in &self.matching_engines {
            let mut engine = adapter.get_engine_mut();
            if engine.venue == venue && engine.instrument().id() == order.instrument_id() {
                let command = nautilus_model::orders::CancelOrder {
                    client_order_id: order.client_order_id(),
                    instrument_id: order.instrument_id(),
                };
                engine.process_cancel(&command, order.account_id());
                return Ok(());
            }
        }
        anyhow::bail!("No matching engine found for venue {}", venue);
    }

    fn modify_order(
        &self,
        _order: &OrderAny,
        _price: Option<Price>,
        _quantity: Option<Quantity>,
    ) -> anyhow::Result<()> {
        // Basic simulation might not support modification yet
        anyhow::bail!("Modify order not implemented in SimExecutionClient");
    }

    fn register(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Proxy for SimExecutionClient to allow shared ownership between ExecutionEngine and BacktestRunner.
pub struct SimExecutionClientProxy {
    client: Rc<SimExecutionClient>,
}

impl SimExecutionClientProxy {
    pub fn new(client: Rc<SimExecutionClient>) -> Self {
        Self { client }
    }
}

impl ExecutionClient for SimExecutionClientProxy {
    fn trader_id(&self) -> TraderId {
        self.client.trader_id()
    }
    fn submit_order(&self, order: &OrderAny) -> anyhow::Result<()> {
        self.client.submit_order(order)
    }
    fn cancel_order(&self, order: &OrderAny) -> anyhow::Result<()> {
        self.client.cancel_order(order)
    }
    fn modify_order(
        &self,
        order: &OrderAny,
        price: Option<Price>,
        quantity: Option<Quantity>,
    ) -> anyhow::Result<()> {
        self.client.modify_order(order, price, quantity)
    }
    fn register(&self) -> anyhow::Result<()> {
        self.client.register()
    }
}

pub struct BacktestRunner {
    clock: Rc<RefCell<TestClock>>,
    cache: Rc<RefCell<Cache>>,
    portfolio: Rc<RefCell<Portfolio>>,
    execution_engine: Rc<RefCell<ExecutionEngine>>,
    sim_client: Rc<SimExecutionClient>,
    trader_id: TraderId,
}

impl BacktestRunner {
    pub fn new(trader_id: TraderId, account_id: AccountId, venue: Venue) -> anyhow::Result<Self> {
        let clock = Rc::new(RefCell::new(TestClock::new(
            UnixNanos::from(0),
            trader_id.clone().into(),
        )));
        let cache = Rc::new(RefCell::new(Cache::new(trader_id.clone().into())));

        // Initialize account
        let usd = Currency::from("USD");
        let balance = AccountBalance::new(
            Money::new(100_000.0, usd),
            Money::new(0.0, usd),
            Money::new(100_000.0, usd),
        );
        let account_state = AccountState::new(
            account_id.clone(),
            AccountType::Cash,
            vec![balance],
            vec![],
            true, // is_reported
            UUID4::new(),
            clock.borrow().now(),
            clock.borrow().now(),
            Some(usd),
        );
        let account = AccountAny::Cash(CashAccount::new(account_state, true, false));
        cache.borrow_mut().add_account(account)?;

        let portfolio = Rc::new(RefCell::new(Portfolio::new(
            cache.clone(),
            clock.clone() as Rc<RefCell<dyn Clock>>,
            None,
        )));

        let mut sim_client = SimExecutionClient::new(trader_id.clone());
        let execution_engine = Rc::new(RefCell::new(ExecutionEngine::new(
            trader_id.clone(),
            clock.clone() as Rc<RefCell<dyn Clock>>,
            cache.clone(),
        )));

        sim_client.set_execution_engine(execution_engine.clone());
        let sim_client = Rc::new(sim_client);

        execution_engine.borrow_mut().register_execution_client(
            Box::new(SimExecutionClientProxy::new(sim_client.clone())),
            vec![venue],
        )?;

        // Register msgbus handlers
        execution_engine.borrow_mut().register_msgbus_handlers();

        Ok(Self {
            clock,
            cache,
            portfolio,
            execution_engine,
            sim_client,
            trader_id,
        })
    }

    pub fn add_instrument(&mut self, instrument: InstrumentAny) {
        self.cache.borrow_mut().add_instrument(instrument.clone());

        let adapter = OrderEngineAdapter::new(
            instrument,
            0, // raw_id
            FillModelAny::Default,
            FeeModelAny::MakerTakerDefault,
            BookType::L1_MBP,
            OmsType::Netting,
            AccountType::Cash,
            self.clock.clone() as Rc<RefCell<dyn Clock>>,
            self.cache.clone(),
            nautilus_execution::matching_engine::config::OrderMatchingEngineConfig::default(),
        );

        Rc::get_mut(&mut self.sim_client)
            .expect("Cannot add matching engine: SimExecutionClient is shared")
            .add_matching_engine(adapter);
    }

    pub fn run(&mut self, instrument_id: InstrumentId, data: DataFrame) -> anyhow::Result<()> {
        let ts_event_col = data.column("ts_event")?.u64()?;
        let open_col = data.column("open")?.f64()?;
        let high_col = data.column("high")?.f64()?;
        let low_col = data.column("low")?.f64()?;
        let close_col = data.column("close")?.f64()?;
        let volume_col = data.column("volume")?.f64()?;

        let bar_type = BarType::Standard {
            instrument_id,
            spec: BarSpecification::new(1, BarAggregation::Minute, PriceType::Last),
            aggregation_source: AggregationSource::External,
        };

        let endpoint = MessagingSwitchboard::exec_engine_process();

        for i in 0..data.height() {
            let ts_event = UnixNanos::from(ts_event_col.get(i).unwrap());
            let open = open_col.get(i).unwrap();
            let high = high_col.get(i).unwrap();
            let low = low_col.get(i).unwrap();
            let close = close_col.get(i).unwrap();
            let volume = volume_col.get(i).unwrap();

            // Advance clock
            self.clock.borrow_mut().set_time(ts_event);

            let bar = Bar {
                bar_type,
                open: Price::new(open, 8),
                high: Price::new(high, 8),
                low: Price::new(low, 8),
                close: Price::new(close, 8),
                volume: Quantity::new(volume, 8),
                ts_event,
                ts_init: ts_event,
            };

            // Publish bar to message bus (for strategies)
            msgbus::publish_bar(endpoint, bar.clone());

            // Process bar in matching engine (for order execution)
            self.sim_client.process_bar(&bar);

            // Drain trading commands (from strategies)
            drain_trading_cmd_queue(self.trader_id.clone().into());
        }

        Ok(())
    }
}
