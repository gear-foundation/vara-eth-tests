#![no_std]
#![allow(clippy::new_without_default)]
#![allow(unused_imports)]
#![allow(static_mut_refs)]
use core::fmt::Debug;
use core::str::FromStr;
use sails_rs::{
    client::Actor,
    collections::HashMap,
    gstd::{msg, service},
    prelude::*,
};

pub mod funcs;
pub mod utils;

static mut STORAGE: Option<Storage> = None;

#[derive(Debug, Default)]
pub struct Storage {
    balances: HashMap<ActorId, U256>,
    allowances: HashMap<(ActorId, ActorId), U256>,
    meta: Metadata,
    total_supply: U256,
}

impl Storage {
    pub fn get_mut() -> &'static mut Self {
        unsafe { STORAGE.as_mut().expect("Storage is not initialized") }
    }
    pub fn get() -> &'static Self {
        unsafe { STORAGE.as_ref().expect("Storage is not initialized") }
    }
    pub fn balances() -> &'static mut HashMap<ActorId, U256> {
        let storage = unsafe { STORAGE.as_mut().expect("Storage is not initialized") };
        &mut storage.balances
    }
    pub fn total_supply() -> &'static mut U256 {
        let storage = unsafe { STORAGE.as_mut().expect("Storage is not initialized") };
        &mut storage.total_supply
    }
}

#[derive(Debug, Default)]
pub struct Metadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u16,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub enum Event {
    Approval {
        owner: String,
        spender: String,
        value: String,
    },
    Transfer {
        from: String,
        to: String,
        value: String,
    },
}

#[derive(Clone)]
pub struct Service;

impl Service {
    pub fn new() -> Self {
        Self
    }

    pub fn init(name: String, symbol: String, decimals: u16) -> Self {
        unsafe {
            STORAGE = Some(Storage {
                meta: Metadata {
                    name,
                    symbol,
                    decimals,
                },
                ..Default::default()
            });
        }
        Self
    }
}

#[service(events = Event)]
impl Service {
    #[export]
    pub fn approve(&mut self, spender: ActorId, value: String) -> bool {
        let owner = msg::source();
        let storage = Storage::get_mut();
        let value_u256 = parse_u256(&value);
        let mutated = funcs::approve(&mut storage.allowances, owner, spender, value_u256);

        if mutated {
            self.emit_event(Event::Approval {
                owner: owner.to_string(),
                spender: spender.to_string(),
                value,
            })
            .expect("Notification Error");
        }

        mutated
    }

    #[export]
    pub fn transfer(&mut self, to: ActorId, value: String) -> bool {
        let from = msg::source();
        let storage = Storage::get_mut();
        let value_u256 = parse_u256(&value);
        let mutated =
            utils::panicking(move || funcs::transfer(&mut storage.balances, from, to, value_u256));

        if mutated {
            self.emit_event(Event::Transfer {
                from: from.to_string(),
                to: to.to_string(),
                value,
            })
            .expect("Notification Error");
        }

        mutated
    }

    #[export]
    pub fn transfer_from(&mut self, from: ActorId, to: ActorId, value: String) -> bool {
        let spender = msg::source();
        let storage = Storage::get_mut();
        let value_u256 = parse_u256(&value);
        let mutated = utils::panicking(move || {
            funcs::transfer_from(
                &mut storage.allowances,
                &mut storage.balances,
                spender,
                from,
                to,
                value_u256,
            )
        });

        if mutated {
            self.emit_event(Event::Transfer { from: from.to_string(), to: to.to_string(), value })
                .expect("Notification Error");
        }

        mutated
    }

    #[export]
    pub fn allowance(&self, owner: ActorId, spender: ActorId) -> String {
        let storage = Storage::get();
        funcs::allowance(&storage.allowances, owner, spender).to_string()
    }

    #[export]
    pub fn balance_of(&self, account: ActorId) -> String {
        let storage = Storage::get();
        funcs::balance_of(&storage.balances, account).to_string()
    }

    #[export]
    pub fn decimals(&self) -> u16 {
        let storage = Storage::get();
        storage.meta.decimals
    }

    #[export]
    pub fn name(&self) -> String {
        let storage = Storage::get();
        storage.meta.name.clone()
    }

    #[export]
    pub fn symbol(&self) -> String {
        let storage = Storage::get();
        storage.meta.symbol.clone()
    }

    #[export]
    pub fn total_supply(&self) -> String {
        let storage = Storage::get();
        storage.total_supply.to_string()
    }
}

fn parse_u256(amount: &str) -> U256 {
    U256::from_dec_str(amount).unwrap_or_else(|_| panic!("Invalid U256 decimal string"))
}
