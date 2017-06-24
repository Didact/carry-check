extern crate futures;
#[macro_use]
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;

use std::io::{self, Write};
use futures::{Future, Stream};
use hyper::{Client, Request, Method};
use hyper_tls::{HttpsConnector};
use tokio_core::reactor::{Core};

header! { (XBnetApiHeader, "X-API-Key") => [String] }

fn main() {
    let mut core = Core::new().unwrap();
    let handle = &core.handle();
    let client = Client::configure()
    	.connector(HttpsConnector::new(4, handle).unwrap())
    	.build(handle);

    let uri = "https://www.bungie.net/Platform/Destiny/Explorer/Items/".parse().unwrap();
    let mut req = Request::new(Method::Get, uri);
    req.headers_mut().set(XBnetApiHeader("REPLACE_ME".to_owned()));
    let work = client.request(req).and_then(|res| {
    	println!("Response: {}", res.status());

    	res.body().for_each(|chunk| {
    		io::stdout()
    			.write_all(&chunk)
    			.map_err(From::from)
    	})
    });
    core.run(work).unwrap();
}