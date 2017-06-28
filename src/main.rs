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
use serde_json::{Value};
use std::result::{Result};
use std::option::{Option};
use std::boxed::{Box};

header! { (XBnetApiHeader, "X-API-Key") => [String] }

const API_KEY: &'static str = "";

#[derive(Copy, Clone, Debug)]
enum PlatformType {
    Xbl = 1,
    Psn = 2,
    Unknown = 254
}

#[derive(Clone, Debug)]
struct AccountId(String);
impl Display for AccountId {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug)]
struct CharacterId(String);
impl Display for CharacterId {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

fn get_account_id<CC>(platform: PlatformType, display_name: &str, client: &Client<CC>) -> Box<Future<Item=Option<AccountId>, Error=hyper::Error>>
where CC: hyper::client::Connect {
    let uri = format!("https://www.bungie.net/Platform/Destiny/SearchDestinyPlayer/{membershipType}/{displayName}/", membershipType=platform as u8, displayName=display_name);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.into()));
    Box::new(client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            serde_json::from_slice::<Value>(&body)
                .as_ref().ok()
                .and_then(|v| v.get("Response"))
                .and_then(|r| r.as_array())
                .and_then(|a| a.first())
                .and_then(|r| r.get("membershipId"))
                .and_then(|m| m.as_str())
                .map(String::from)
                .map(|id| AccountId(id))
        })
    }))
}

fn get_character_ids<CC>(platform: PlatformType, account: &AccountId, client: &Client<CC>) -> Box<Future<Item=Vec<CharacterId>, Error=hyper::Error>>
where CC: hyper::client::Connect {
    let uri = format!("https://www.bungie.net/Platform/Destiny/{membershipType}/Account/{destinyMembershipId}/Summary/", membershipType=platform as u8, destinyMembershipId=account);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.into()));
    Box::new(client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            serde_json::from_slice::<Value>(&body)
                .as_ref().ok()
                .and_then(|v| v.get("Response")).and_then(|v| v.get("data")).and_then(|v| v.get("characters"))
                .and_then(|v| v.as_array())
                .map(|vs| vs.iter().filter_map(|cv| {
                    cv
                        .get("characterBase")
                        .and_then(|cv| cv.get("characterId"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .map(|id| CharacterId(id))
                }))
                .map(|ids| ids.collect())
                .unwrap_or(vec![])
        })
    }))
}

fn main() {
    let mut core = Core::new().unwrap();
    let handle = &core.handle();
    let client = Client::configure()
    	.connector(HttpsConnector::new(4, handle).unwrap())
    	.build(handle);

    let work = get_account_id(PlatformType::Psn, "King_Cepheus", &client).and_then(|id| {
        get_character_ids(PlatformType::Psn, &id.unwrap(), &client).map(|ids| {
            println!("{:?}", ids);
        })
    });
    core.run(work).unwrap();
}