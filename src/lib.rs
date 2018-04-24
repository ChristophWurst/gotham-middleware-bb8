extern crate bb8;
extern crate bb8_postgres;
extern crate futures;
extern crate gotham;
#[macro_use]
extern crate gotham_derive;
#[macro_use]
extern crate log;
extern crate tokio_core;

use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process;
use std::sync::{Arc, Mutex};

use bb8::{ManageConnection, Pool};
use futures::future::{ok, Future};
use gotham::handler::HandlerFuture;
use gotham::middleware::{Middleware, NewMiddleware};
use gotham::state::{FromState, State};
use tokio_core::reactor::Handle;

pub struct Bb8Middleware<M, F>
where
    M: ManageConnection,
    F: Fn() -> M + Send + Sync + 'static,
{
    mgr: AssertUnwindSafe<Arc<Box<F>>>,
    pool: AssertUnwindSafe<Arc<Mutex<Option<Pool<M>>>>>,
}

impl<M, F> Bb8Middleware<M, F>
where
    M: ManageConnection,
    F: Fn() -> M + Send + Sync + 'static,
{
    pub fn new(mgr_f: F) -> Self {
        Bb8Middleware {
            mgr: AssertUnwindSafe(Arc::new(Box::new(mgr_f))),
            pool: AssertUnwindSafe(Arc::new(Mutex::new(None))),
        }
    }

    fn new_with_pool(mgr_f: Arc<Box<F>>, pool: Arc<Mutex<Option<Pool<M>>>>) -> Self {
        Bb8Middleware {
            mgr: AssertUnwindSafe(mgr_f),
            pool: AssertUnwindSafe(pool),
        }
    }
}

impl<M, F> Middleware for Bb8Middleware<M, F>
where
    M: ManageConnection,
    F: Fn() -> M + Send + Sync + 'static,
{
    fn call<Chain>(self, mut state: State, chain: Chain) -> Box<HandlerFuture>
    where
        Chain: FnOnce(State) -> Box<HandlerFuture> + 'static,
    {
        let get_pool = {
            let remote = { Handle::borrow_from(&state).remote().clone() };
            let opt_pool = (*self.pool).lock().unwrap();
            let mgr = (self.mgr)();

            match *opt_pool {
                Some(ref pool) => Box::new(ok(pool.clone())),
                None => Pool::builder().max_size(100).build(mgr, remote),
            }
        };

        Box::new(get_pool.map_err(|_e| unimplemented!()).and_then(|pool| {
            let mwd = Bb8MiddlewareData::new(pool);
            state.put(mwd);

            chain(state)
        }))
    }
}

impl<M, F> NewMiddleware for Bb8Middleware<M, F>
where
    M: ManageConnection,
    F: Fn() -> M + Send + Sync + 'static,
{
    type Instance = Bb8Middleware<M, F>;

    fn new_middleware(&self) -> io::Result<Self::Instance> {
        match catch_unwind(|| self.pool.clone()) {
            Ok(pool_arc) => match catch_unwind(|| self.mgr.clone()) {
                Ok(mgr_f) => Ok(Bb8Middleware::new_with_pool(mgr_f, pool_arc)),
                Err(_) => panic!("nooooooo"),
            },
            Err(_) => {
                error!("PANIC: bb8::Pool::clone caused a panic");
                eprintln!("PANIC: bb8::Pool::clone caused a panic");
                process::abort()
            }
        }
    }
}

#[derive(StateData)]
pub struct Bb8MiddlewareData<M: ManageConnection> {
    pool: Pool<M>,
}

impl<M> Bb8MiddlewareData<M>
where
    M: ManageConnection,
{
    pub fn new(pool: Pool<M>) -> Self {
        Self { pool: pool }
    }

    pub fn pool(&self) -> &Pool<M> {
        &self.pool
    }
}
