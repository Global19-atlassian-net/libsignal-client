//
// Copyright 2020-2021 Signal Messenger, LLC.
// SPDX-License-Identifier: AGPL-3.0-only
//

use libsignal_protocol::*;
use neon::prelude::*;
use libsignal_bridge::node;
use signal_neon_futures::*;
use async_trait::async_trait;
use neon::context::Context;
use std::sync::Arc;
use std::fmt;
use std::marker::PhantomData;
use std::panic::AssertUnwindSafe;

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

pub fn boxed_object<'a, T: 'static + Send>(cx: &mut TaskContext<'a>, value: T) -> JsResult<'a, JsValue> {
    Ok(cx.boxed(node::DefaultFinalize(value)).upcast())
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

    async fn do_get_sender_key(&self, name: SenderKeyName) -> Result<Option<SenderKeyRecord>, String> {
        eprintln!("here...");
        let store_object_shared = self.store_object.clone();
        JsFuture::get_promise(&self.js_queue, move |cx| {
            let store_object = store_object_shared.to_inner(cx);
            let name : Handle::<JsValue> = boxed_object(cx, name.clone())?; // XXX clone
            eprintln!("now here...");
            let result = call_method(cx, store_object, "_getSenderKey", std::iter::once(name))?;
            eprintln!("result = {:?}", result.to_string(cx)?.value(cx));
            let result = result.downcast_or_throw(cx)?;
            store_object_shared.finalize(cx);
            eprintln!("did something...");
            Ok(result)
        })
            .then(|cx, result| {

                  eprintln!("got a result...");
                  match result {

            Ok(value) => {
                match value.downcast::<JsObject, _>(cx) {
                    Ok(obj) => {
                        eprintln!("obj {:?}", obj.to_string(cx).map_err(|e| e.to_string())?.value(cx));
                        let handle = call_method(cx, obj, "_unsafeGetNativeHandle", std::iter::empty()).map_err(|e| e.to_string())?.downcast_or_throw(cx);

                        let handle : Handle::<JsObject> = handle.map_err(|e| e.to_string())?;

                        eprintln!("handle {:?}", (*handle).to_string(cx).map_err(|e| e.to_string())?.value(cx));
                        Ok(None) // fixme!
                    }
                    Err(_) => {
                        eprintln!("not an object?");
                        if value.is_a::<JsNull, _>(cx) {
                            eprintln!("you returned null");
                            Ok(None)
                        } else {
                            eprintln!("not an object");
                            Err("result must be an object".into())
                        }
                    }
                }
            },
                      Err(error) => {

                          eprintln!("downcast failed :/");
                          Err(error
                .to_string(cx)
                .expect("can convert to string")
                .value(cx))
                      }
                  }})
        .await
    }

    async fn do_save_sender_key(&self, name: SenderKeyName, record: SenderKeyRecord) -> Result<(), String> {
        let store_object_shared = self.store_object.clone();
        JsFuture::get_promise(&self.js_queue, move |cx| {
            let store_object = store_object_shared.to_inner(cx);
            let name : Handle::<JsValue> = boxed_object(cx, name)?;
            let record : Handle::<JsValue> = boxed_object(cx, record)?;
            let result = call_method(cx, store_object, "_saveSenderKey", std::iter::once(name).chain(std::iter::once(record)))?
                .downcast_or_throw(cx)?;
            store_object_shared.finalize(cx);
            Ok(result)
        })
        .then(|cx, result| match result {
            Ok(value) => match value.downcast::<JsUndefined, _>(cx) {
                Ok(_) => Ok(()),
                Err(_) => Err("name must be a string".into()),
            },
            Err(error) => Err(error
                .to_string(cx)
                .expect("can convert to string")
                .value(cx)),
        })
        .await
        }
}

impl<'a> Finalize for NodeSenderKeyStore<'a> {
    fn finalize<'b, C: neon::prelude::Context<'b>>(self, cx: &mut C) {
        self.store_object.finalize(cx)
    }
}

#[async_trait(?Send)]
impl<'a> SenderKeyStore for NodeSenderKeyStore<'a> {
    async fn load_sender_key(
        &mut self,
        sender_key_name: &SenderKeyName,
        _ctx: libsignal_protocol::Context,
    ) -> Result<Option<SenderKeyRecord>, SignalProtocolError> {
        eprintln!("load_sender_key({:?})", sender_key_name);
        // XXX clone()
        self.do_get_sender_key(sender_key_name.clone()).await.map_err(|s| js_error_to_rust("getSenderKey", s))
    }

    async fn store_sender_key(
        &mut self,
        sender_key_name: &SenderKeyName,
        record: &SenderKeyRecord,
        _ctx: libsignal_protocol::Context,
    ) -> Result<(), SignalProtocolError> {
        eprintln!("store_sender_key({:?})", sender_key_name);
        // XXX clone()
        self.do_save_sender_key(sender_key_name.clone(), record.clone()).await.map_err(|s| js_error_to_rust("saveSenderKey", s))
    }
}

#[allow(non_snake_case)]
#[doc = "ts: export function SenderKeyDistributionMessage_Create(name: SenderKeyName, store: Object): Promise<SenderKeyDistributionMessage>"]
pub fn SenderKeyDistributionMessage_Create(mut cx: FunctionContext) -> JsResult<JsObject> {

    // XXX There must be an easier way than this:
    let name_arg = cx.argument::<<&SenderKeyName as node::ArgTypeInfo>::ArgType>(0)?;
    let mut name_borrow = <&SenderKeyName as node::ArgTypeInfo>::borrow(&mut cx, name_arg)?;
    let name = <&SenderKeyName as node::ArgTypeInfo>::load_from(&mut cx, &mut name_borrow)?;
    let store_arg = cx.argument(1)?;
    let mut store = NodeSenderKeyStore::new(&mut cx, store_arg);

    let name = name.clone(); // XXX clone

    promise(&mut cx, async move {

        let mut rng = rand::rngs::OsRng;
        let future = AssertUnwindSafe(create_sender_key_distribution_message(&name, &mut store, &mut rng, None));
        let result = future.await;
        settle_promise(move |cx| {
            store.finalize(cx);
            match result {
                Ok(obj) => Ok(cx.boxed(node::DefaultFinalize(obj))),
                Err(e) => {
                    let s = e.to_string();
                    cx.throw_error::<String, neon::prelude::Handle::<JsBox<_>>>(s)
                }
            }
        })
    })
}

#[allow(non_snake_case)]
#[doc = "ts: export function SenderKeyDistributionMessage_Process(name: SenderKeyName, msg: SenderKeyDistributionMessage, store: Object): Promise<void>"]
pub fn SenderKeyDistributionMessage_Process(mut cx: FunctionContext) -> JsResult<JsObject> {
    eprintln!("process skdm");
    // XXX There must be an easier way than this:
    let name_arg = cx.argument::<<&SenderKeyName as node::ArgTypeInfo>::ArgType>(0)?;
    let mut name_borrow = <&SenderKeyName as node::ArgTypeInfo>::borrow(&mut cx, name_arg)?;
    let name = <&SenderKeyName as node::ArgTypeInfo>::load_from(&mut cx, &mut name_borrow)?;
    let name = name.clone(); // XXX clone
    eprintln!("got name {:?}", name);

    eprintln!("getting arg 2...");
    let skdm_arg = cx.argument(1)?;
    eprintln!("got arg1");

    //let skdm_arg = cx.argument::<<&SenderKeyDistributionMessage as node::ArgTypeInfo>::ArgType>(1)?;
    eprintln!("got arg");
    eprintln!("got arg");
    let mut skdm_borrow = <&SenderKeyDistributionMessage as node::ArgTypeInfo>::borrow(&mut cx, skdm_arg)?;
    eprintln!("got borrow");
    let skdm = <&SenderKeyDistributionMessage as node::ArgTypeInfo>::load_from(&mut cx, &mut skdm_borrow)?;
    eprintln!("got load_from");
    let skdm = skdm.clone(); // XXX clone
    eprintln!("got skdm");

    let store_arg = cx.argument(2)?;
    let mut store = NodeSenderKeyStore::new(&mut cx, store_arg);

    promise(&mut cx, async move {
        let future = AssertUnwindSafe(process_sender_key_distribution_message(&name, &skdm, &mut store, None));
        let result = future.await;
        settle_promise(move |cx| {
            store.finalize(cx);
            match result {
                Ok(obj) => Ok(cx.boxed(node::DefaultFinalize(obj))),
                Err(e) => {
                    let s = e.to_string();
                    cx.throw_error::<String, neon::prelude::Handle::<JsBox<_>>>(s)
                }
            }
        })
    })
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    libsignal_bridge::node::register(&mut cx)?;
    cx.export_function("initLogger", logging::init_logger)?;
    cx.export_function("SenderKeyDistributionMessage_Create", SenderKeyDistributionMessage_Create)?;
    cx.export_function("SenderKeyDistributionMessage_Process", SenderKeyDistributionMessage_Process)?;
    Ok(())
}
