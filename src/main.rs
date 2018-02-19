#![feature(const_fn)]
#![feature(universal_impl_trait)]
#![feature(conservative_impl_trait)]
#![feature(nll)]
#![feature(proc_macro)]
#![feature(generators)]

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
// #[macro_use]
// extern crate mdo;
// #[macro_use]
// extern crate mdo_future;

extern crate futures_await as futures;
extern crate hyper_tls;
extern crate tokio_core;
extern crate serde_json;
extern crate chrono;
extern crate num;

use std::fmt::{Display, Formatter, Error};
use std::io::{self, Write};
use std::result::{Result};
use std::option::{Option};
use std::io::{BufRead};
use std::str::{FromStr};
use std::sync::{Arc};

use futures::{Future, Stream};
use futures::prelude::{async, await, async_block};

use hyper::{Client, Request, Method};
use hyper::client::{Connect};
use hyper_tls::{HttpsConnector};

use tokio_core::reactor::{Core};

use serde_json::{Value};

use chrono::{DateTime, Utc};

use num::cast::{FromPrimitive};

header! { (XBnetApiHeader, "X-API-Key") => [String] }

lazy_static! {
    static ref API_KEY: String = {
        std::env::var("BNET_API").unwrap()
    };
}

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


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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

const ACCOUNT_ID_KEY: MemoizeKey<(PlatformType, String), Option<AccountId>, hyper::Error> = MemoizeKey::new("get_account_id");
#[async]
fn get_account_id(platform: PlatformType, display_name: String, client: Arc<Client<impl Connect>>) -> hyper::Result<Option<AccountId>> {
    // println!("Getting AccountId for {:?}", display_name);
    let uri = format!("https://www.bungie.net/Platform/Destiny2/SearchDestinyPlayer/{membershipType}/{displayName}/", membershipType=platform as u8, displayName=display_name);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
    await!(memoize(ACCOUNT_ID_KEY, (platform, display_name), client.request(req).and_then(|res| {
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
    })))
}

const CHARACTER_IDS_KEY: MemoizeKey<(PlatformType, AccountId), Vec<CharacterId>, hyper::Error> = MemoizeKey::new("get_character_ids");
#[async]
fn get_character_ids(platform: PlatformType, account_id: AccountId, client: Arc<Client<impl Connect>>) -> hyper::Result<Vec<CharacterId>> {
    // println!("Getting CharacterIds for {:?}", account_id);
    let uri = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Profile/{destinyMembershipId}/?components=200", membershipType=platform as u8, destinyMembershipId=account_id);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
    await!(memoize(CHARACTER_IDS_KEY, (platform, account_id), client.request(req).and_then(|res| {
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
    })))
}

const TRIALS_GAME_IDS_KEY: MemoizeKey<(PlatformType, AccountId, CharacterId), Vec<PgcrId>, hyper::Error> = MemoizeKey::new("get_trials_game_ids");
#[async]
fn get_trials_game_ids(platform: PlatformType, account: AccountId, character: CharacterId, client: Arc<Client<impl Connect>>) -> hyper::Result<Vec<PgcrId>> {
    // println!("Getting PgcrIDs for {:?}", character);
    let uri = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/Activities/?count=5&mode=39", 
        membershipType=platform as u8, 
        destinyMembershipId = account, 
        characterId = character);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
    await!(memoize(TRIALS_GAME_IDS_KEY, (platform, account, character), client.request(req).and_then(bind!([character], |res: hyper::Response<_>| {
        res.body().concat2().map(move |body| {
            let games = serde_json::from_slice::<Value>(&body)
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
                .unwrap_or(vec![]);
            println!("Games for id: {}: {:?}", character, games);
            games
        })
    }))
    ))
}

#[derive(Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
enum Team {
        Alpha,
        Bravo
}

#[derive(Debug, Clone, Copy)]
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
       let kills =  get_stat("kills", values).unwrap();
       let deaths = get_stat("deaths", values).unwrap();
       let assists = get_stat("assists", values).unwrap();
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

const CARNAGE_REPORT_KEY: MemoizeKey<PgcrId, Pgcr, hyper::Error> = MemoizeKey::new("get_carnage_report");
#[async]
fn get_carnage_report(pgcr_id: PgcrId, client: Arc<Client<impl Connect>>) -> hyper::Result<Pgcr> {
    // println!("Getting Pgcr for {:?}", pgcr_id);
    let url = format!("https://www.bungie.net/Platform/Destiny2/Stats/PostGameCarnageReport/{id}/", id=pgcr_id);
    let mut req = Request::new(Method::Get, url.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
    await!(memoize(CARNAGE_REPORT_KEY, pgcr_id, client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            // io::stdout().write_all(&body);
            serde_json::from_slice::<Value>(&body).as_ref().ok().map(Pgcr::from_value).unwrap().unwrap()
            // serde_json::from_slice::<Pgcr>(&body).unwrap()
        })
    })))
}

fn get_stat<T>(key: &str, value: &Value) -> Option<T>
where
    T: FromPrimitive {
        value.get(key).and_then(|v| v.get("basic")).and_then(|v| v.get("value")).and_then(|v| v.as_f64()).and_then(T::from_f64)
}

const ACCOUNT_STATS_KEY: MemoizeKey<(PlatformType, AccountId), PlayerInstanceStats, hyper::Error> = MemoizeKey::new("get_account_stats");
#[async]
fn get_account_stats(platform: PlatformType, account_id: AccountId, client: Arc<Client<impl Connect>>) -> hyper::Result<PlayerInstanceStats> {
    // println!("Getting PlayerInstanceStats for {:?}", account_id);
    let character_ids = await!(get_character_ids(platform, account_id, client.clone()))?;
    for id in character_ids {
        let url = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/?modes=39", membershipType=platform as u8, destinyMembershipId=account_id, characterId=id);
        let mut req = Request::new(Method::Get, url.parse().unwrap());
        req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
        let results = await!(
            memoize(ACCOUNT_STATS_KEY, (platform, account_id), client.request(req).and_then(|res| {
            res.body().concat2().map(|body| {
                io::stdout().write_all(&body);
                io::stdout().write_all(&['\n' as u8]);
                let stats = serde_json::from_slice::<Value>(&body)
                    .as_ref().ok()
                    .and_then(|v| v.get("Response"))
                    .and_then(|v| v.get("trialsofthenine"))
                    .and_then(|v| v.get("allTime"))
                    .map(|stats_base| {
                        (
                            get_stat::<u64>("kills", stats_base).unwrap(),
                            get_stat::<u64>("assists", stats_base).unwrap(),
                            get_stat::<u64>("deaths", stats_base).unwrap(),
                        )
                    });
                stats.unwrap();
                // println!("{:?}", stats.unwrap());
                // println!("here");
                PlayerInstanceStats{assists: 0, deaths: 0, id: AccountId(20), kills: 0, team: Team::Alpha}
            })
        }))
        );
    }

    Ok(PlayerInstanceStats{assists: 0, deaths: 0, id: AccountId(20), kills: 0, team: Team::Alpha})
        
}

const ACCOUNT_NAME_KEY: MemoizeKey<(PlatformType, AccountId), String, hyper::Error> = MemoizeKey::new("get_account_name");
#[async]
fn get_account_name(platform: PlatformType, account_id: AccountId, client: Arc<Client<impl Connect>>) -> hyper::Result<String> {
    let url = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Profile/{destinyMembershipId}/?components=100",membershipType=platform as u8, destinyMembershipId=account_id);
    let mut req = Request::new(Method::Get, url.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
    await!(memoize(ACCOUNT_NAME_KEY, (platform, account_id), client.request(req).and_then(move |res| {
             res.body().concat2().map(move |body| {
                 let gamertag: String = serde_json::from_slice::<Value>(&body)
                    .as_ref().ok()
                    .and_then(|v| v.get("Response"))
                    .and_then(|v| v.get("profile"))
                    .and_then(|v| v.get("data"))
                    .and_then(|v| v.get("userInfo"))
                    .and_then(|v| v.get("displayName"))
                    .and_then(|v| v.as_str())
                    .map(From::from)
                    .unwrap();
                gamertag
            })
    })))
}

#[async]
fn get_elo(account_id: AccountId, client: Arc<Client<impl Connect>>) -> hyper::Result<f64> {
    let url = format!("https://api.guardian.gg/v2/players/{}?lc=en", account_id);
    println!("{}", url);
    let req = Request::new(Method::Get, url.parse().unwrap());
    await!(client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            serde_json::from_slice::<Value>(&body)
                .as_ref().ok()
                .and_then(|v| v.get("player"))
                .and_then(|v| v.get("stats"))
                .and_then(|v| v.as_array())
                .and_then(|a| a.iter().find(|v| v.as_object().unwrap().get("mode").unwrap().as_u64().unwrap() == 39))
                .and_then(|v| v.as_object())
                .and_then(|v| v.get("elo"))
                .and_then(|e| e.as_f64())
                .unwrap()
        })
    }))
}

fn main() {
    let mut core = Core::new().unwrap();
    let handle = &core.handle();
    let client = Arc::new(Client::configure()
    	.connector(HttpsConnector::new(4, handle).unwrap())
    	.build(handle));

    let stdin = std::io::stdin();
    println!("Go ahead");
    let gamertag = stdin.lock().lines().next().unwrap().unwrap();

        let work = async_block! {
            let account_id = await!(get_account_id(PlatformType::Psn, gamertag, client.clone())).unwrap().unwrap();
            let character_ids = await!(get_character_ids(PlatformType::Psn, account_id, client.clone())).unwrap();
            let mut all_pgcr_ids = vec![];
            for character_id in character_ids {
                let pgcr_ids = await!(get_trials_game_ids(PlatformType::Psn, account_id, character_id, client.clone())).unwrap();
                all_pgcr_ids.extend(pgcr_ids.iter());
            }
            let pgcr = await!(get_carnage_report(all_pgcr_ids[1], client.clone()));
            let elo = await!(get_elo(account_id, client.clone()));
            elo
        };

            // let all_trials_game_ids: Vec<PgcrId> = vec![];
            // for character_id in character_ids {
            //     let trials_game_ids = await!(get_trials_game_ids(PlatformType::Psn, account_id, character_id, client.clone())).unwrap();
            //     all_trials_game_ids.append(&mut trials_game_ids);
            // }
        // let work = get_account_id(PlatformType::Psn, gamertag, client.clone()).and_then(|id| {
        //     println!("{:?}", id.unwrap());
        //     get_character_ids(PlatformType::Psn, id.unwrap(), &client).and_then(bind!([id, client = client.clone()], |ids: Vec<CharacterId>| {
        //         join_all(ids.into_iter().map(bind!([id, client = client.clone()], |char_id| {
        //             get_trials_game_ids(PlatformType::Psn, id.unwrap(), char_id, &client)
        //         })))
        //         .map(|pgcr_ids| pgcr_ids.into_iter().flat_map(|x| x.into_iter()).collect::<Vec<_>>())
        //         .map(bind!([client = client.clone()], |pgcr_ids: Vec<PgcrId>| {
        //             futures_unordered(pgcr_ids.into_iter().map(|pgcr_id| {
        //                 get_carnage_report(pgcr_id, &client)
        //             }))
        //         }))
        //         .and_then(bind!([client = client.clone()], |pgcrs: futures::stream::FuturesUnordered<_>| {
        //         // diverge to hopefully concurrent processing
        //             pgcrs.and_then(bind!([client = client.clone()], |pgcr: Pgcr| {
        //                 let my_team: Team = pgcr.stats.iter().find(|&&stat| stat.id == id.unwrap()).unwrap().team;
        //                 let stats = pgcr.stats.clone();
        //                 join_all(stats.into_iter().filter(|stat: &PlayerInstanceStats| stat.team != my_team).collect::<Vec<_>>().into_iter().map(bind!([pgcr = pgcr.clone(), client = client.clone()], |stat: PlayerInstanceStats| {
        //                     get_account_name(PlatformType::Psn, stat.id, &client).map(bind!([pgcr = pgcr.clone(), stat], |name| {
        //                         format!("Game {}, Player {}: {:.*}", pgcr.time, name, 2, stat.kda())
        //                     }))
        //                 })))
        //             })).collect()
        //         }))
            // }))
        // });
        // println!("----");
        let stats = core.run(work).unwrap();
        println!("{:?}", stats);
        // for game in stats {
        //     for s in game {
        //         println!("{:?}", s);
        //     }
        //     println!("====");
        // }
}
