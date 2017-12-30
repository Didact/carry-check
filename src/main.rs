#![feature(const_fn)]
#![feature(conservative_impl_trait)]

#[macro_use]
mod bind;
mod memoize;

use memoize::{memoize, MemoizeKey};

#[macro_use] 
extern crate hyper;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate lazy_static;

extern crate futures;
extern crate hyper_tls;
extern crate tokio_core;
extern crate serde_json;
extern crate chrono;
extern crate num;

use std::fmt::{Display, Formatter, Error};
use std::io::{self, Write};
use std::result::{Result};
use std::option::{Option};
use std::boxed::{Box};
use std::str::{FromStr};
use std::sync::{Arc};

use futures::{Future, Stream};
use futures::future::{join_all};
use futures::stream::{futures_unordered};

use hyper::{Client, Request, Method};
use hyper_tls::{HttpsConnector};

use tokio_core::reactor::{Core};

use serde_json::{Value};

use chrono::{DateTime, Utc};

use num::cast::{FromPrimitive};

header! { (XBnetApiHeader, "X-API-Key") => [String] }

const API_KEY: &'static str = "";

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
enum PlatformType {
    Xbl = 1,
    Psn = 2,
    Unknown = 254
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct AccountId(u64);
impl Display for AccountId {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}


#[derive(Copy, Clone, Debug)]
struct CharacterId(u64);
impl Display for CharacterId {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
struct PgcrId(u64);
impl Display for PgcrId {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

const account_id_key: MemoizeKey<(PlatformType, &str), Option<AccountId>, hyper::Error> = MemoizeKey::new("get_account_id");
fn get_account_id<'a, CC>(platform: PlatformType, display_name: &'a str, client: &Client<CC>) -> impl Future<Item=Option<AccountId>, Error=hyper::Error> +'a
where CC: hyper::client::Connect {
    let uri = format!("https://www.bungie.net/Platform/Destiny2/SearchDestinyPlayer/{membershipType}/{displayName}/", membershipType=platform as u8, displayName=display_name);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.into()));
    memoize(account_id_key, (platform, display_name), client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            serde_json::from_slice::<Value>(&body)
                .as_ref().ok()
                .and_then(|v| v.get("Response"))
                .and_then(|r| r.as_array())
                .and_then(|a| a.first())
                .and_then(|r| r.get("membershipId"))
                .and_then(|m| m.as_str())
                .map(FromStr::from_str)
                .map(|id| AccountId(id.unwrap()))
                //TODO
                //.unwrap()
        })
    }))
}

const character_ids_key: MemoizeKey<(PlatformType, AccountId), Vec<CharacterId>, hyper::Error> = MemoizeKey::new("get_character_ids");
fn get_character_ids<CC>(platform: PlatformType, account_id: AccountId, client: &Client<CC>) -> impl Future<Item=Vec<CharacterId>, Error=hyper::Error>
where CC: hyper::client::Connect {
    let uri = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Profile/{destinyMembershipId}/?components=200", membershipType=platform as u8, destinyMembershipId=account_id);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.into()));
    memoize(character_ids_key, (platform, account_id), client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            serde_json::from_slice::<Value>(&body)
                .as_ref().ok()
                .and_then(|v| v.get("Response")).and_then(|v| v.get("characters")).and_then(|v| v.get("data"))
                .and_then(|v| v.as_object())
                .map(|v| v.keys())
                .map(|ks| ks.into_iter().map(|k| u64::from_str(k).unwrap()))
                .map(|ids| ids.into_iter().map(|id| CharacterId(id)))
                .map(|ids| ids.collect())
                .unwrap_or(vec![])
               
        })
    }))
}

const trials_game_ids_key: MemoizeKey<(PlatformType, AccountId), Vec<PgcrId>, hyper::Error> = MemoizeKey::new("get_trials_game_ids");
fn get_trials_game_ids<CC>(platform: PlatformType, account: AccountId, character: CharacterId, client: &Client<CC>) -> impl Future<Item=Vec<PgcrId>, Error=hyper::Error>
where CC: hyper::client::Connect {
    let uri = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/Activities/?count=1&mode=39", 
        membershipType=platform as u8, 
        destinyMembershipId = account, 
        characterId = character);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.into()));
    memoize(trials_game_ids_key, (platform, account), client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            serde_json::from_slice::<Value>(&body)
                .as_ref().ok()
                .and_then(|v| v.get("Response")).and_then(|v| v.get("activities"))
                .and_then(|v| v.as_array())
                .map(|vs| vs.iter().filter_map(|cv| {
                    cv
                        .get("activityDetails").and_then(|cv| cv.get("instanceId"))
                        .and_then(|v| v.as_str())
                        .map(FromStr::from_str)
                        .map(|id| id.unwrap())
                        .map(|id| PgcrId(id))
                }))
                .map(|ids| ids.collect())
                .unwrap_or(vec![])
        })
    }))
}

#[derive(Deserialize, Debug, Copy, Clone)]
enum Team {
        Alpha,
        Bravo
}

#[derive(Debug, Clone)]
struct PlayerInstanceStats {
    id: AccountId,
    kills: u64,
    deaths: u64,
    assists: u64,
    team: Team
}


impl PlayerInstanceStats {

    fn kd(&self) -> f64 {
        self.kills as f64 / self.deaths as f64
    }

    fn kda(&self) -> f64 {
        (self.kills + self.assists) as f64 / self.deaths as f64
    }

    fn from_value(value: &serde_json::Value) -> Result<PlayerInstanceStats, serde_json::error::Error> {
       
       let id = value.get("player").and_then(|v| v.get("destinyUserInfo")).and_then(|v| v.get("membershipId")).and_then(|v| v.as_str()).map(FromStr::from_str).map(|id| AccountId(id.unwrap())).unwrap();
       let values = value.get("values").unwrap();
       let kills = values.get("kills").and_then(|v| v.get("basic")).and_then(|v| v.get("value")).and_then(|v| v.as_f64()).unwrap() as u64;
       let deaths = values.get("deaths").and_then(|v| v.get("basic")).and_then(|v| v.get("value")).and_then(|v| v.as_f64()).unwrap() as u64;
       let assists = values.get("assists").and_then(|v| v.get("basic")).and_then(|v| v.get("value")).and_then(|v| v.as_f64()).unwrap() as u64;
       let team = if values.get("team").and_then(|v| v.get("basic")).and_then(|v| v.get("value")).and_then(|v| v.as_f64()).unwrap() == 16.0 { Team::Alpha } else { Team::Bravo };
        Ok(PlayerInstanceStats{
            id:  id,
            kills: kills,
            deaths: deaths,
            assists: assists,
            team: team
        })
    }
}

#[derive(Debug, Clone)]
struct Pgcr {
    time: DateTime<Utc>,
    stats: Vec<PlayerInstanceStats>,
    winner: Team
}

impl Pgcr {
    fn from_value(value: &serde_json::Value) -> Result<Pgcr, serde_json::error::Error> {
        Ok(Pgcr{
            time: serde_json::from_value(value.get("Response").and_then(|v| v.get("period")).unwrap().clone()).unwrap(),
            stats: value.get("Response").and_then(|v| v.get("entries")).unwrap().as_array().unwrap().into_iter().map(PlayerInstanceStats::from_value).filter_map(|r| r.ok()).collect(),
            winner: if value.get("Response").and_then(|v| v.get("teams")).and_then(|v| v.as_array()).map(|vs| vs.iter().find(|v| v.get("standing").unwrap().get("basic").unwrap().get("value").unwrap().as_f64().unwrap() == 0.0).unwrap().get("teamId").unwrap().as_f64().unwrap()).unwrap() == 16.0 { Team::Alpha } else { Team::Bravo } 
        })
    }
}

const carnage_report_key: MemoizeKey<PgcrId, Pgcr, hyper::Error> = MemoizeKey::new("get_carnage_report");
fn get_carnage_report<CC>(pgcr_id: PgcrId, client: &Client<CC>) -> impl Future<Item=Pgcr, Error=hyper::Error>
where CC: hyper::client::Connect {
    let url = format!("https://www.bungie.net/Platform/Destiny2/Stats/PostGameCarnageReport/{id}/", id=pgcr_id);
    let mut req = Request::new(Method::Get, url.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.into()));
    memoize(carnage_report_key, pgcr_id, client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            serde_json::from_slice::<Value>(&body).as_ref().ok().map(Pgcr::from_value).unwrap().unwrap()
            // serde_json::from_slice::<Pgcr>(&body).unwrap()
        })
    }))
}

fn get_stat<T>(key: &str, value: &Value) -> Option<T>
where
    T: FromPrimitive {
        value.get(key).and_then(|v| v.get("basic")).and_then(|v| v.get("value")).and_then(|v| v.as_f64()).and_then(T::from_f64)
}

const account_stats_key: MemoizeKey<(PlatformType, AccountId), PlayerInstanceStats, hyper::Error> = MemoizeKey::new("get_account_stats");
fn get_account_stats<'a, CC>(platform: PlatformType, account_id: AccountId, client: &Client<CC>) -> impl Future<Item=PlayerInstanceStats, Error=hyper::Error> + 'a
where 
    CC: hyper::client::Connect,
    Client<CC>: Clone {
    get_character_ids(platform, account_id, client).and_then(bind!([client = client.clone()], |ids: Vec<CharacterId>| {
        let url = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/?modes=39", membershipType=platform as u8, destinyMembershipId=account_id, characterId=ids[0]);
        let mut req = Request::new(Method::Get, url.parse().unwrap());
        req.headers_mut().set(XBnetApiHeader(API_KEY.into()));
        memoize(account_stats_key, (platform, account_id), client.request(req).and_then(|res| {
            res.body().concat2().map(|body| {
                io::stdout().write_all(&body);
                io::stdout().write_all(&['\n' as u8]);
                let stats = serde_json::from_slice::<Value>(&body)
                    .as_ref().ok()
                    .and_then(|v| v.get("Response"))
                    .and_then(|v| v.get("trialsofthenine"))
                    .and_then(|v| v.get("allTime"))
                    .and_then(|stats_base| {
                        (get_stat::<u64>("kills", stats_base))
                    });
                stats.unwrap();
                println!("here");

                unimplemented!()
            })
        }))
    }))
}

fn main() {
    let mut core = Core::new().unwrap();
    let handle = &core.handle();
    let client = Arc::new(Client::configure()
    	.connector(HttpsConnector::new(4, handle).unwrap())
    	.build(handle));

    let gamertag = "King_Cepheus";

    let work = get_account_id(PlatformType::Psn, gamertag, &client).and_then(|id| {
        get_character_ids(PlatformType::Psn, id.unwrap(), &client).and_then(bind!([id, client = client.clone()], |ids: Vec<CharacterId>| {
            join_all(ids.into_iter().map(bind!([id, client = client.clone()], |char_id| {
                get_trials_game_ids(PlatformType::Psn, id.unwrap(), char_id, &client)
            })))
            .map(|pgcr_ids| pgcr_ids.into_iter().flat_map(|x| x.into_iter()).collect::<Vec<_>>())
            .map(bind!([client = client.clone()], |pgcr_ids: Vec<PgcrId>| {
                futures_unordered(pgcr_ids.into_iter().map(|pgcr_id| {
                    get_carnage_report(pgcr_id, &client)
                }))
            }))
            .and_then(bind!([client = client.clone()], |pgcrs: futures::stream::FuturesUnordered<_>| {
                // diverge to hopefully concurrent processing
                pgcrs.and_then(bind!([client = client.clone()], |pgcr: Pgcr| {
                    join_all(pgcr.stats.into_iter().map(bind!([client = client.clone()], |stat: PlayerInstanceStats| {
                        get_account_stats(PlatformType::Psn, stat.id, &client)
                    })))
                })).collect()
            }))
        }))
    });
    

    core.run(work).unwrap();
}