extern crate bb8;
extern crate bb8_postgres;
extern crate env_logger;
extern crate futures;
extern crate futures_state_stream;
extern crate gotham;
extern crate gotham_middleware_bb8;
extern crate hyper;
extern crate mime;
extern crate tokio_core;
extern crate tokio_postgres;

use std::error::Error;
use std::thread;

use bb8::Builder as Bb8Builder;
use bb8_postgres::PostgresConnectionManager;
use futures::Future;
use futures_state_stream::StateStream;
use gotham::handler::HandlerFuture;
use gotham::http::response::create_response;
use gotham::pipeline::new_pipeline;
use gotham::pipeline::single::single_pipeline;
use gotham::router::Router;
use gotham::router::builder::*;
use gotham::state::{FromState, State};
use gotham_middleware_bb8::{Bb8Middleware, Bb8MiddlewareData};
use hyper::StatusCode;
use tokio_core::reactor::{Core, Remote};

pub fn say_hello(state: State) -> Box<HandlerFuture> {
    let f = {
        let pool = Bb8MiddlewareData::<PostgresConnectionManager>::borrow_from(&state).pool();

        pool.run(|connection| {
            connection
                .prepare("SELECT 1")
                .and_then(|(select, connection)| {
                    connection.query(&select, &[]).for_each(|row| {
                        println!("result: {}", row.get::<i32, usize>(0 as usize));
                    })
                })
                .and_then(|connection| Ok(("hello".to_owned(), connection)))
        })
    }.then(|res| {
        let text = match res {
            Ok(text) => text,
            Err(err) => format!("Error: {}", err.description()),
        };

        let res = create_response(
            &state,
            StatusCode::Ok,
            Some((text.into_bytes(), mime::TEXT_PLAIN)),
        );

        Ok((state, res))
    });

    Box::new(f)
}

fn router(remote: Remote) -> Box<Future<Item = Router, Error = ()>> {
    let conn_mgr = PostgresConnectionManager::new(
        "postgresql://postgres:mysecretpassword@localhost:5432",
        || tokio_postgres::TlsMode::None,
    ).unwrap();
    let f = Bb8Builder::new()
        .build(conn_mgr, remote)
        .and_then(|pool| {
            // docker run --name gotham-middleware-postgres -e POSTGRES_PASSWORD=mysecretpassword -p 5432:5432 -d postgres
            let (chain, pipelines) =
                single_pipeline(new_pipeline().add(Bb8Middleware::new(pool)).build());

            let router = build_router(chain, pipelines, |route| {
                route.get("/").to(say_hello);
            });

            Ok(router)
        })
        .map_err(|_| ());

    Box::new(f)
}

pub fn main() {
    env_logger::init();

    let addr = "127.0.0.1:7878";
    let mut core = Core::new().unwrap();

    let remote = core.remote();

    core.run(router(remote).and_then(|router| {
        println!("Listening for requests at http://{}", addr);

        thread::spawn(move || {
            gotham::start(addr, router);
        }).join()
            .unwrap();

        Ok(())
    })).unwrap();
}
