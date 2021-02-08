//
// Copyright 2020-2021 Signal Messenger, LLC.
// SPDX-License-Identifier: AGPL-3.0-only
//

use libsignal_protocol::{SenderKeyRecord, SenderKeyName, SenderKeyStore, SenderKeyDistributionMessage};
use libsignal_protocol::{PublicKey, SignalProtocolError, Context as SignalContext};
use neon::prelude::*;
use libsignal_bridge::node;
//use signal_neon_futures::*;
use async_trait::async_trait;
use std::sync::Arc;
use std::fmt;
use std::marker::PhantomData;

pub mod logging;

#[derive(Debug)]
struct CallbackError {
    message: String,
}

impl CallbackError {
    fn new(message: String) -> CallbackError {
        Self { message }
    }
}

impl fmt::Display for CallbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "callback error {}", self.message)
    }
}

impl std::error::Error for CallbackError {}

fn js_error_to_rust(func: &'static str, err: String) -> SignalProtocolError {
    SignalProtocolError::ApplicationCallbackError(func, Box::new(CallbackError::new(err)))
}

struct NodeSenderKeyStore<'a> {
    js_queue: EventQueue,
    store_object: Arc<Root<JsObject>>,
    phantom: PhantomData<&'a ()>,
}

impl<'a> NodeSenderKeyStore<'a> {
    fn new(cx: &mut FunctionContext, store: Handle<JsObject>) -> Self {
        Self {
            js_queue: cx.queue(),
            store_object: Arc::new(store.root(cx)),
            phantom: PhantomData,
        }
    }

    async fn do_get_sender_key(&self, name: &SenderKeyName) -> Result<Option<SenderKeyRecord>, String> {

        /*
        let store_object_shared = self.store_object.clone();
        JsFuture::get_promise(&self.js_queue, move |cx| {
            let store_object = store_object_shared.to_inner(cx);
            let result = call_method(cx, store_object, "_getSenderKey", &[name])?
                .downcast_or_throw(cx)?;
            store_object_shared.finalize(cx);
            Ok(result)
        })
        .then(|cx, result| match result {
            Ok(value) => match value.downcast::<JsObject, _>(cx) {
                Ok(obj) => {
                    let handle : JsValue = *call_method(cx, obj, "_unsafeGetNativeHandle", std::iter::empty())?;

                    Ok(None)
                }
                Err(_) => Err("result must be an object".into()),
            },
            Err(error) => Err(error
                .to_string(cx)
                .expect("can convert to string")
                .value(cx)),
        })
        .await
         */

        Ok(None)
    }

    async fn do_save_sender_key(&self, name: &SenderKeyName, record: &SenderKeyRecord) -> Result<(), String> {

        /*
        let store_object_shared = self.store_object.clone();
        JsFuture::get_promise(&self.js_queue, move |cx| {
            let store_object = store_object_shared.to_inner(cx);
            let result = call_method(cx, store_object, "_saveSenderKey", &[name, record])?
                .downcast_or_throw(cx)?;
            store_object_shared.finalize(cx);
            Ok(result)
        })
        .then(|cx, result| match result {
            Ok(value) => match value.downcast::<JsObject, _>(cx) {
                Ok(s) => Ok(s),
                Err(_) => Err("name must be a string".into()),
            },
            Err(error) => Err(error
                .to_string(cx)
                .expect("can convert to string")
                .value(cx)),
        })
        .await
         */

        Ok(())
    }
}

impl<'a> Finalize for NodeSenderKeyStore<'a> {
    fn finalize<'b, C: Context<'b>>(self, cx: &mut C) {
        self.store_object.finalize(cx)
    }
}

#[async_trait(?Send)]
impl<'a> SenderKeyStore for NodeSenderKeyStore<'a> {
    async fn load_sender_key(
        &mut self,
        sender_key_name: &SenderKeyName,
        _ctx: SignalContext,
    ) -> Result<Option<SenderKeyRecord>, SignalProtocolError> {
        self.do_get_sender_key(&sender_key_name).await.map_err(|s| js_error_to_rust("getSenderKey", s))
    }

    async fn store_sender_key(
        &mut self,
        sender_key_name: &SenderKeyName,
        record: &SenderKeyRecord,
        _ctx: SignalContext,
    ) -> Result<(), SignalProtocolError> {
        self.do_save_sender_key(&sender_key_name, record).await.map_err(|s| js_error_to_rust("saveSenderKey", s))
    }
}

#[allow(non_snake_case)]
#[doc = "ts: export function SenderKeyDistributionMessage_Create(name: SenderKeyName, store: Object): SenderKeyDistributionMessage"]
pub fn SenderKeyDistributionMessage_Create(mut cx: FunctionContext) -> JsResult<JsValue> {

    // XXX There must be an easier way than this:
    let name_arg = cx.argument::<<&SenderKeyName as node::ArgTypeInfo>::ArgType>(0)?;
    let mut name_borrow = <&SenderKeyName as node::ArgTypeInfo>::borrow(&mut cx, name_arg)?;
    let name = <&SenderKeyName as node::ArgTypeInfo>::load_from(&mut cx, &mut name_borrow)?;

    let store_arg = cx.argument(1)?;
    let store = NodeSenderKeyStore::new(&mut cx, store_arg);

    eprintln!("SenderKeyDistributionMessage_Create {:?}", name);
    fn create_sender_key_distribution_message() -> Result<SenderKeyDistributionMessage, SignalProtocolError> {
        let chain_key = vec![32; 32];
        let pk_buf = vec![0x05; 33];
        let signing_key = PublicKey::deserialize(pk_buf.as_ref())?;
        SenderKeyDistributionMessage::new(1, 2, &chain_key, signing_key)
    };

    node::return_boxed_object(&mut cx, create_sender_key_distribution_message())
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    libsignal_bridge::node::register(&mut cx)?;
    cx.export_function("initLogger", logging::init_logger)?;
    cx.export_function("SenderKeyDistributionMessage_Create", SenderKeyDistributionMessage_Create)?;
    Ok(())
}
