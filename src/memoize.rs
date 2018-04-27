extern crate futures;

use std::sync::{Arc, Mutex};
use std::collections::{HashMap};
use std::hash::{Hash};
use futures::future::{Future, IntoFuture};
use std::marker::{PhantomData};

lazy_static! {
    static ref MEMOIZE_DICT: Mutex<HashMap<&'static str, usize>> = {Mutex::new(HashMap::new())};
}

#[derive(Copy, Clone)]
pub struct MemoizeKey<K, V, E>  
where 
    K: Hash + Eq + Clone,
    V: Clone,
{
    key: &'static str,
    key_type: PhantomData<*const K>,
    value_type: PhantomData<*const V>,
    error_type: PhantomData<*const E>
}

impl <K, V, E> MemoizeKey<K, V, E> 
where 
    K: Hash + Eq + Clone,
    V: Clone,
{
    pub const fn new(key: &'static str) -> MemoizeKey<K, V, E> {
        MemoizeKey {
            key: key,
            key_type: PhantomData,
            value_type: PhantomData,
            error_type: PhantomData
        }
    }

}

pub fn memoize<K, T, E, F>(memoize_key: MemoizeKey<K, T, E>, primary_key: K, future: F) -> Memoize<K, T, E, F::Future> 
where 
    K: Hash + Eq + Clone,
    T: Clone,
    F: IntoFuture<Item=T, Error=E> {
        let mut dict = MEMOIZE_DICT.lock().unwrap();
        if !dict.contains_key(memoize_key.key) {
            dict.insert(
                memoize_key.key, 
                Box::into_raw(Box::new(Arc::new(Mutex::new(HashMap::<K, T>::new())))) as usize
            );
        }

        Memoize {
            type_identifier: memoize_key.key,
            key: primary_key,
            cache: unsafe { &*(*dict.get(memoize_key.key).unwrap() as *mut Arc<Mutex<HashMap<K, T>>>) }.clone(),
            future: future.into_future()
        }
}

pub struct Memoize<K, T, E, F>
where F: Future<Item=T, Error=E> {
    type_identifier: &'static str,
    key: K,
    cache: Arc<Mutex<HashMap<K, T>>>,
    future: F
}

impl <K, T, E, F> Future for Memoize<K, T, E, F>
where 
    K: Hash + Eq + Clone,
    T: Clone,
    F: Future<Item=T, Error=E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        let mut dict = self.cache.lock().unwrap();
        if let Some(cached) = dict.get(&self.key) {
            trace!("Found cached value for {}", self.type_identifier);
            return Ok(futures::Async::Ready(cached.clone()))
        }
        let result = self.future.poll();
        match result {
            Ok(futures::Async::Ready(ref t)) => {
                trace!("Insert cached value for {}", self.type_identifier);
                dict.insert(self.key.clone(), t.clone());
            }
            _ => {}
        }
        result
       
    }
}
