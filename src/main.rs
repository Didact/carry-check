#![feature(const_fn)]
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

extern crate futures_await as futures;
extern crate hyper_tls;
extern crate tokio_core;
extern crate serde_json;
extern crate chrono;
extern crate num;

use std::fmt::{Display, Formatter, Error};
use std::result::{Result};
use std::option::{Option};
use std::str::{FromStr};
use std::collections::{HashMap};

use futures::{Future, Stream};
use futures::prelude::{async, await, async_stream, stream_yield};

use hyper::{Client, Request, Method, StatusCode};
use hyper::client::{Connect};
use hyper::server::{Service, Response};
use hyper_tls::{HttpsConnector};

use tokio_core::reactor::{Core};

use serde_json::{Value};

use chrono::{DateTime, Utc};

use num::cast::{FromPrimitive};
use std::sync::{Mutex};

#[derive(Copy, Clone, Debug)]
struct InferenceInput {
    account_id: AccountId,
    kd: f64,
    total_games: u64,
    elo: f64,
}

struct Carry {
    handle: tokio_core::reactor::Handle,
}

impl Service for Carry {
    type Request = Request;
    type Response = Response<Box<Stream<Item=hyper::Chunk, Error=Self::Error>>>;
    type Error = hyper::Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let client = Client::configure()
            .connector(HttpsConnector::new(4, &self.handle).unwrap())
            .build(&self.handle);
        // println!("requested: {:?}", req.uri());
        let mut response = Response::new();
        match (req.method(), req.path().split('/').nth(1)) {
            (&Method::Get, Option::Some("search")) => {

            }
            _ => {
                // println!("{:?}", b);
                response.set_status(StatusCode::NotFound);
                return Box::new(futures::future::ok(response));
            }
        };
        let gamertag = req.uri().path().split('/').nth(2).unwrap();
        let results = run_full(String::from(gamertag), client.clone());
        match req.uri().path().split('/').nth(3) {
            Some("stream") => {
                Box::new(futures::future::ok(Response::new().with_body(results_to_chunks(results))))
            }
            _ => {
                Box::new(results.collect().map(|results| {
                    let vec = serde_json::to_vec(&results).unwrap();
                    let other: hyper::Chunk = vec.into();
                    let stream: Box<Stream<Item=hyper::Chunk, Error=hyper::Error>> = Box::new(futures::stream::once(Ok::<hyper::Chunk, hyper::Error>(other)));
                    Response::new().with_body(stream)
                }))
            }
        }
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
fn get_trials_game_ids(platform: PlatformType, account: AccountId, character: CharacterId, count: u64, client: Client<impl Connect>) -> hyper::Result<Vec<PgcrId>> {
    // println!("Getting PgcrIDs for {:?}", character);
    let uri = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/Activities/?count={count}&mode=39", 
        membershipType=platform as u8, 
        destinyMembershipId = account, 
        characterId = character,
        count = count);
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
    let mut player_stats = PlayerStats{account_id, assists: 0, deaths: 0, kills: 0, total_games: 0};
    for character_id in character_ids {
        let url = format!("https://www.bungie.net/Platform/Destiny2/{membershipType}/Account/{destinyMembershipId}/Character/{characterId}/Stats/?modes=39", membershipType=platform as u8, destinyMembershipId=account_id, characterId=character_id);
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
            player_stats.total_games += stats.3;
            player_stats.kills += stats.0;
            player_stats.assists = stats.1;
            player_stats.deaths += stats.2;
    }
    Ok(player_stats)
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

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all="camelCase")]
struct WeeklyInfo {
    // membership_id: AccountId,
    mode: u64,
    elo: u64,
    // date: DateTime<Utc>,
    kills: u64,
    deaths: u64,
    // #[serde(rename="gamesPlayed")]
    games_played: u64,
    wins: u64,
}

const ELO_KEY: MemoizeKey<AccountId, f64, hyper::Error> = MemoizeKey::new("get_elo");
#[async]
fn get_elo(account_id: AccountId, client: Client<impl Connect>) -> hyper::Result<f64> {    
    let url = format!("https://api.guardian.gg/v2/players/{}/performance/39/2017-09-06/{}?lc=en", account_id, Utc::now().format("%Y-%m-%d"));
    // println!("{}", url);
    let req = Request::new(Method::Get, url.parse().unwrap());
    await!(memoize(ELO_KEY, account_id, client.request(req).and_then(|res| {
        res.body().concat2().map(|body| {
            let weeklies = serde_json::from_slice::<Vec<WeeklyInfo>>(&body).unwrap();
            let last_elo = weeklies.last().unwrap().elo;
            last_elo as f64
        })
    })))
}

#[async]
fn get_inference_input(platform: PlatformType, account_id: AccountId, client: Client<impl Connect>) -> hyper::Result<InferenceInput> {
    let lifetime_stats = await!(get_account_stats(platform, account_id, client.clone()))?;
    let elo = await!(get_elo(account_id, client.clone()))?;
    Ok(InferenceInput{account_id, kd: lifetime_stats.kills as f64 / lifetime_stats.deaths as f64, total_games: lifetime_stats.total_games, elo: elo})
}

fn make_carry_judgement<'a>(inputs: &Vec<InferenceInput>, classifiers: &HashMap<&'a str, &Fn(&Vec<InferenceInput>) -> Vec<AccountId>>) -> Vec<(AccountId, Vec<&'a str>)> {
    let mut reasons = HashMap::new();
    for (&name, f) in classifiers {
        let results = f(inputs);
        if !results.is_empty() {
            for result in results {
                reasons.entry(result).or_insert(vec![]).push(name);
            }
        }
    }
    return reasons.into_iter().map(|(k, v)| (k, v)).collect();
}

fn kd_fn(input: &Vec<InferenceInput>) -> Vec<AccountId> {
    let mut max_kd = std::f64::MIN;
    let mut min_kd = std::f64::MAX;
    for i in input {
        max_kd = max_kd.max(i.kd);
        min_kd = min_kd.min(i.kd);
    }
    if max_kd < 0.0 {
        // something went wrong
        return vec![]
    }
    println!("max kd {:?}", max_kd);
    println!("min kd {:?}", min_kd);
    input.iter().filter(|i| i.kd > 1.5 * min_kd).map(|i| i.account_id).collect()
}

fn games_fn(input: &Vec<InferenceInput>) -> Vec<AccountId> {
    let mut max_games = u64::min_value();
    let mut min_games = u64::max_value();
    for i in input {
        max_games = std::cmp::max(max_games, i.total_games);
        min_games = std::cmp::min(min_games, i.total_games);

    }
    if max_games == 0 {
        // something went wrong
        return vec![]
    }
    println!("max games {:?}", max_games);
    println!("min games {:?}", min_games);
    if min_games < 50 && max_games > 100 {
        // everyone over 100 is a carrier
        return input.iter().filter(|i| i.total_games > 100).map(|i| i.account_id).collect()
    }
    vec![]
}

fn elo_fn(input: &Vec<InferenceInput>) -> Vec<AccountId> {
    let mut max_elo = std::f64::MIN;
    let mut min_elo = std::f64::MAX;
    for i in input {
        max_elo = max_elo.max(i.elo);
        min_elo = max_elo.min(i.elo);
    }
    return input.iter().filter(|i| (i.elo - min_elo) > 300.0).map(|i| i.account_id).collect();
}

#[derive(Debug, Clone, Serialize)]
struct FullResult {
    opponents: Vec<String>,
    judgements: HashMap<String, Vec<&'static str>>,
    time: DateTime<Utc>,
    num_categories: usize,
    won: bool,
}

#[async_stream(item = FullResult)]
fn run_full(gamertag: String, client: Client<impl Connect>) -> hyper::Result<()> {

    let account_id = await!(get_account_id(PlatformType::Psn, gamertag, client.clone()))?.unwrap();
    let character_ids = await!(get_character_ids(PlatformType::Psn, account_id, client.clone()))?;
    // println!("{:?}", character_ids);
    let mut games = vec![];
    for character_id in character_ids {
        // println!("{:?}", character_id);
        let gs = await!(get_trials_game_ids(PlatformType::Psn, account_id, character_id, 25, client.clone()))?;
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
        if opponents_ids.is_empty() {
            continue;
        }

        let mut gamertags_map = HashMap::new();

        let mut opponents_gamertags = vec![];
        let mut inputs = vec![];

        // println!("{:?}", opponents_ids);
        for id in opponents_ids {
            let gamertag = await!(get_account_name(PlatformType::Psn, id, client.clone())).unwrap();
            gamertags_map.insert(id, gamertag.clone());
            // println!("{:?}", gamertag);
            // println!("{:?}", opponents_gamertags);
            let input = await!(get_inference_input(PlatformType::Psn, id, client.clone()))?;
            // println!("{:?}", input);
            inputs.push(input);
            opponents_gamertags.push(gamertag);
        }

        // println!("----");

        let mut classifiers: HashMap<&'static str, &Fn(&Vec<InferenceInput>) -> Vec<AccountId>> = HashMap::new();
        // classifiers.insert("test", &tttt);
        classifiers.insert("games", &games_fn);
        classifiers.insert("ELO", &elo_fn);
        classifiers.insert("kd", &kd_fn);
        let judgements = make_carry_judgement(&inputs, &classifiers).into_iter().map(|(k, v)| (gamertags_map[&k].clone(), v)).collect();
        stream_yield!(FullResult{opponents: opponents_gamertags, judgements, time: pgcr.time, won: pgcr.winner == my_team, num_categories: classifiers.len()});
        println!("iteration");

    }

    Ok(())

}

fn results_to_chunks<'a>(stream: impl Stream<Item=FullResult, Error=hyper::Error> + 'a) -> Box<Stream<Item=hyper::Chunk, Error=hyper::Error> + 'a> {
    let start = std::rc::Rc::new(Utc::now());
    Box::new(stream.map(|result| {
        println!("*** {:?}", result);
        (serde_json::to_string(&result).unwrap() + "`").into()
    }).map_err(|err| {
        unimplemented!()
    }).inspect(move |_| {
        let current = Utc::now();
        println!("{:?}", current.signed_duration_since(*start.clone()));
    }))
}

fn main() {
    let addr = "0.0.0.0:10302".parse().unwrap();
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
