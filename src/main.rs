extern crate futures;
#[macro_use] extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;
extern crate serde_json;

use std::fmt::{Display, Formatter, Error};
use std::io::{self, Write};
use futures::{Future, Stream};
use hyper::{Client, Request, Method};
use hyper::client::{FutureResponse};
use hyper_tls::{HttpsConnector};
use tokio_core::reactor::{Core};
use serde_json::{Deserializer, Value};
use std::result::{Result};

header! { (XBnetApiHeader, "X-API-Key") => [String] }

#[derive(Copy, Clone, Debug)]
enum PlatformType {
    Xbl = 1,
    Psn = 2,
    Unknown = 254
}

impl Display for PlatformType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            &PlatformType::Xbl => {write!(f, "1")}
            &PlatformType::Psn => {write!(f, "2")}
            &PlatformType::Unknown => {write!(f, "254")}
        }
    }
}

struct AccountId(String);
impl Display for AccountId {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

struct CharacterId(String);
impl Display for CharacterId {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}
fn get_account_summary<CC>(platform: PlatformType, account: &AccountId, client: &Client<CC>) -> FutureResponse 
where CC: hyper::client::Connect {
    let uri = format!("https://www.bungie.net/Platform/Destiny/{membershipType}/Account/{destinyMembershipId}/Summary/", membershipType=platform, destinyMembershipId=account);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(String::from("")));
    client.request(req)
}

fn main() {
    let mut core = Core::new().unwrap();
    let handle = &core.handle();
    let client = Client::configure()
    	.connector(HttpsConnector::new(4, handle).unwrap())
    	.build(handle);

    let work = get_account_summary(PlatformType::Psn, &AccountId("4611686018428490898".to_owned()), &client).and_then(|res| {
        res.body().concat2().map(|body| {
            io::stdout()
        })
    });
    core.run(work).unwrap();
}