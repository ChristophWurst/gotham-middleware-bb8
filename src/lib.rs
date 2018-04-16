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

use bb8::{ManageConnection, Pool};
use gotham::handler::HandlerFuture;
use gotham::middleware::{Middleware, NewMiddleware};
use gotham::state::State;

pub struct Bb8Middleware<M: ManageConnection> {
    pool: AssertUnwindSafe<Pool<M>>,
}

impl<M: ManageConnection> Bb8Middleware<M> {
    pub fn new(pool: Pool<M>) -> Self {
        Bb8Middleware {
            pool: AssertUnwindSafe(pool),
        }
    }
}

impl<M> Middleware for Bb8Middleware<M>
where
    M: ManageConnection + Clone,
{
    fn call<Chain>(self, mut state: State, chain: Chain) -> Box<HandlerFuture>
    where
        Chain: FnOnce(State) -> Box<HandlerFuture> + 'static,
        Self: Sized,
    {
        let mwd = match catch_unwind(|| self.pool.clone()) {
            Ok(pool) => Bb8MiddlewareData::new(pool),
            Err(_) => {
                error!(
                    "PANIC: bb8::Pool::clone caused a panic, unable to rescue with a HTTP error"
                );
                eprintln!(
                    "PANIC: bb8::Pool::clone caused a panic, unable to rescue with a HTTP error"
                );
                process::abort()
            }
        };

        state.put(mwd);

        chain(state)
    }
}

impl<M> NewMiddleware for Bb8Middleware<M>
where
    M: ManageConnection + Clone,
{
    type Instance = Bb8Middleware<M>;

    fn new_middleware(&self) -> io::Result<Self::Instance> {
        match catch_unwind(|| self.pool.clone()) {
            Ok(pool) => Ok(Bb8Middleware::new(pool)),
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
