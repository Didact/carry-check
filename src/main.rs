#![feature(const_fn)]
#![feature(universal_impl_trait)]
#![feature(conservative_impl_trait)]
#![feature(nll)]
#![feature(proc_macro)]
#![feature(generators)]
#![feature(never_type)]

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
#[macro_use]
extern crate maplit;

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
use std::str::{FromStr};
use std::collections::{HashMap};

use futures::{Future, Stream};
use futures::prelude::{async, await};

use hyper::{Client, Request, Method, StatusCode};
use hyper::client::{Connect};
use hyper::server::{Service, Response};
use hyper_tls::{HttpsConnector};

use tokio_core::reactor::{Core};

use serde_json::{Value};

use chrono::{DateTime, Utc};

use num::cast::{FromPrimitive};

use maplit::{hashmap};

#[derive(Copy, Clone, Debug)]
struct InferenceInput {
    kd: f64,
    total_games: u64,
    elo: f64,
}

struct Carry {
    handle: tokio_core::reactor::Handle,
}

impl Service for Carry {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let client = Client::configure()
            .connector(HttpsConnector::new(4, &self.handle).unwrap())
            .build(&self.handle);
        println!("requested: {:?}", req.uri());
        let mut response = Response::new();
        match (req.method(), req.path().split('/').nth(1)) {
            (&Method::Get, Option::Some("search")) => {

            }
            (a, b) => {
                println!("{:?}", b);
                response.set_status(StatusCode::NotFound);
                return Box::new(futures::future::ok(response));
            }
        };
        let gamertag = req.uri().path().split('/').nth(2).unwrap();
        println!("gamertag: {:?}", gamertag);
        let results_future = run_full(String::from(gamertag), client.clone());
        Box::new(results_future.map(|results| {
            response.set_body(serde_json::to_vec(&results).unwrap());
            response
        }))
    }

}

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
fn get_account_id(platform: PlatformType, display_name: String, client: Client<impl Connect>) -> hyper::Result<Option<AccountId>> {
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
fn get_character_ids(platform: PlatformType, account_id: AccountId, client: Client<impl Connect>) -> hyper::Result<Vec<CharacterId>> {
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
fn get_trials_game_ids(platform: PlatformType, account: AccountId, character: CharacterId, client: Client<impl Connect>) -> hyper::Result<Vec<PgcrId>> {
    // println!("Getting PgcrIDs for {:?}", character);
    let uri = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/Activities/?count=6&mode=39", 
        membershipType=platform as u8, 
        destinyMembershipId = account, 
        characterId = character);
    let mut req = Request::new(Method::Get, uri.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
    await!(memoize(TRIALS_GAME_IDS_KEY, (platform, account, character), client.request(req).and_then(bind!([_character = character], |res: hyper::Response<_>| {
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
                .map(|ids| ids.skip(1)) // my personal last game ended with DNF
                .map(|ids| ids.collect())
                .unwrap_or(vec![]);
            // println!("Games for id: {}: {:?}", character, games);
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
    account_id: AccountId,
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
            account_id:  id,
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
fn get_carnage_report(pgcr_id: PgcrId, client: Client<impl Connect>) -> hyper::Result<Pgcr> {
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

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
struct PlayerStats {
    account_id: AccountId,
    total_games: u64,
    kills: u64,
    assists: u64,
    deaths: u64,
}

const ACCOUNT_STATS_KEY: MemoizeKey<(PlatformType, AccountId), PlayerStats, hyper::Error> = MemoizeKey::new("get_account_stats");
#[async]
fn get_account_stats(platform: PlatformType, account_id: AccountId, client: Client<impl Connect>) -> hyper::Result<PlayerStats> {
    // println!("Getting PlayerInstanceStats for {:?}", account_id );
    let character_ids = await!(get_character_ids(platform, account_id, client.clone()))?;
    let url = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/?modes=39", membershipType=platform as u8, destinyMembershipId=account_id, characterId=character_ids[0]);
    let mut req = Request::new(Method::Get, url.parse().unwrap());
    req.headers_mut().set(XBnetApiHeader(API_KEY.clone()));
    let res = await!(client.request(req))?;
    let body = await!(res.body().concat2())?;
    // io::stdout().write_all(&body)?;
    // io::stdout().write_all(&['\n' as u8])?;
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
                get_stat::<u64>("activitiesEntered", stats_base).unwrap(),
            )
        }).unwrap_or((0, 0, 0, 0));
    Ok(PlayerStats{account_id: account_id, total_games: stats.3, kills: stats.0, assists: stats.1, deaths: stats.2})
}

const ACCOUNT_NAME_KEY: MemoizeKey<(PlatformType, AccountId), String, hyper::Error> = MemoizeKey::new("get_account_name");
#[async]
fn get_account_name(platform: PlatformType, account_id: AccountId, client: Client<impl Connect>) -> hyper::Result<String> {
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
fn get_elo(account_id: AccountId, client: Client<impl Connect>) -> hyper::Result<f64> {
    Ok(200.0)
    // let url = format!("https://api.guardian.gg/v2/players/{}?lc=en", account_id);
    // println!("{}", url);
    // let req = Request::new(Method::Get, url.parse().unwrap());
    // await!(client.request(req).and_then(|res| {
    //     res.body().concat2().map(|body| {
    //         serde_json::from_slice::<Value>(&body)
    //             .as_ref().ok()
    //             .and_then(|v| v.get("player"))
    //             .and_then(|v| v.get("stats"))
    //             .and_then(|v| v.as_array())
    //             .and_then(|a| a.iter().find(|v| v.as_object().unwrap().get("mode").unwrap().as_u64().unwrap() == 39))
    //             .and_then(|v| v.as_object())
    //             .and_then(|v| v.get("elo"))
    //             .and_then(|e| e.as_f64())
    //             .unwrap()
    //     })
    // }))
}

#[async]
fn get_inference_input(platform: PlatformType, account_id: AccountId, client: Client<impl Connect>) -> hyper::Result<InferenceInput> {
    let lifetime_stats = await!(get_account_stats(platform, account_id, client.clone()))?;
    // println!("stats");
    let elo = await!(get_elo(account_id, client.clone()))?;
    // println!("elo");
    Ok(InferenceInput{kd: lifetime_stats.kills as f64 / lifetime_stats.deaths as f64, total_games: lifetime_stats.total_games, elo: elo})
}

fn make_carry_judgement<'a>(inputs: &Vec<InferenceInput>, classifiers: &HashMap<&'a str, &Fn(&Vec<InferenceInput>) -> bool>) -> Vec<&'a str> {
    let mut reasons = vec![];
    for (name, f) in classifiers {
        let result = f(inputs);
        if result {
            reasons.push(*name);
        }
    }
    reasons
}

fn tttt(input: &Vec<InferenceInput>) -> bool {
    true
}

fn games_fn(input: &Vec<InferenceInput>) -> bool {
    let mut max_games = u64::min_value();
    let mut min_games = u64::max_value();
    for i in input {
        max_games = std::cmp::max(max_games, i.total_games);
        min_games = std::cmp::min(min_games, i.total_games);

    }
    println!("max {:?}", max_games);
    println!("min {:?}", min_games);
    return max_games > (min_games * 1.5 as u64);
}

fn elo_fn(input: &Vec<InferenceInput>) -> bool {
    let mut max_elo = std::f64::MIN;
    let mut min_elo = std::f64::MAX;
    for i in input {
        max_elo = max_elo.max(i.elo);
        min_elo = max_elo.min(i.elo);
    }
    return max_elo > (min_elo * 2 as f64);
}

#[derive(Debug, Clone, Serialize)]
struct FullResult {
    opponents: Vec<String>,
    judgements: Vec<&'static str>,
}

#[async]
fn run_full(gamertag: String, client: Client<impl Connect>) -> hyper::Result<Vec<FullResult>> {
    let mut results = vec![];
    let account_id = await!(get_account_id(PlatformType::Psn, gamertag, client.clone()))?.unwrap();
    let character_ids = await!(get_character_ids(PlatformType::Psn, account_id, client.clone()))?;
    // println!("{:?}", character_ids);
    let mut games = vec![];
    for character_id in character_ids {
        // println!("{:?}", character_id);
        let gs = await!(get_trials_game_ids(PlatformType::Psn, account_id, character_id, client.clone()))?;
        games.extend(gs);
    }
    // println!("{:?}", games);
    let mut pgcrs = vec![];
    for game in games {
        let pgcr = await!(get_carnage_report(game, client.clone()))?;
        pgcrs.push(pgcr);
    }

    // println!("{:?}", pgcrs);

    for pgcr in pgcrs {
        let my_team = pgcr.stats.iter().find(|s| s.account_id == account_id).unwrap().team;
        let other_team = match my_team {
            Team::Alpha => Team::Bravo,
            Team::Bravo => Team::Alpha,
        };

        let opponents_ids = pgcr.stats.iter().filter(|s| s.team == other_team).map(|s| s.account_id).collect::<Vec<_>>();
        let mut opponents_gamertags = vec![];
        let mut inputs = vec![];
        // println!("{:?}", opponents_ids);
        for id in opponents_ids {
            let gamertag = await!(get_account_name(PlatformType::Psn, id, client.clone())).unwrap();
            // println!("{:?}", gamertag);
            // println!("{:?}", opponents_gamertags);
            let input = await!(get_inference_input(PlatformType::Psn, id, client.clone())).expect("inferene input");
            // println!("{:?}", input);
            inputs.push(input);
            opponents_gamertags.push(gamertag);
        }
        // println!("----");

        let mut classifiers: HashMap<&'static str, &Fn(&Vec<InferenceInput>) -> bool> = HashMap::new();
        // classifiers.insert("test", &tttt);
        classifiers.insert("games played", &games_fn);
        classifiers.insert("ELO", &elo_fn);
        let judgements = make_carry_judgement(&inputs, &classifiers);
        results.push(FullResult{opponents: opponents_gamertags, judgements});

    }

    Ok(results)
}

fn main() {
    let addr = "127.0.0.1:3001".parse().unwrap();
    let mut core = Core::new().unwrap();
    let server_handle = core.handle();
    let client_handle = core.handle();
    let serve = hyper::server::Http::new().serve_addr_handle(&addr, &server_handle, move || {
        Ok(Carry{handle: client_handle.clone()})
    }).unwrap();
    let h2 = server_handle.clone();
    server_handle.spawn(serve.for_each(move |conn| {
        h2.spawn(conn.map(|_| ()).map_err(|err| println!("serve err: {:?}", err)));
        Ok(())
    }).map_err(|_| ()));
    core.run(futures::future::empty::<(), ()>()).unwrap();
}
