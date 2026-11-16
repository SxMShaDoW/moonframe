//! Sends a web view's session through the PDF proxy ([`super::proxy`]):
//! `WKWebsiteDataStore.proxyConfigurations` with an HTTP CONNECT proxy on
//! this machine. Both are macOS 14's, so they are looked up as the app runs
//! (an app built for older systems still starts there); without them a web
//! page is not made into a PDF at all.

#![allow(
    unsafe_code,
    reason = "Network framework calls found at run time, and a WebKit setter the bindings lack"
)]

use {
    objc2::{class, msg_send, rc::Retained, runtime::AnyObject, sel},
    objc2_web_kit::WKWebsiteDataStore,
    std::{
        ffi::{CString, c_char, c_int, c_void},
        sync::OnceLock,
    },
};

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_LAZY: c_int = 0x1;

/// `nw_endpoint_create_host(hostname, port)`.
type CreateHost = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut AnyObject;
/// `nw_proxy_config_create_http_connect(endpoint, tls_options)`.
type CreateConnect = unsafe extern "C" fn(*mut AnyObject, *mut AnyObject) -> *mut AnyObject;

/// The Network framework's two functions, when this system has them.
fn functions() -> Option<(CreateHost, CreateConnect)> {
    static FOUND: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    let (host, connect) = (*FOUND.get_or_init(|| {
        // SAFETY: a system framework's path, and symbols looked up in it.
        unsafe {
            let network = dlopen(
                c"/System/Library/Frameworks/Network.framework/Network".as_ptr(),
                RTLD_LAZY,
            );
            if network.is_null() {
                return None;
            }
            let host = dlsym(network, c"nw_endpoint_create_host".as_ptr());
            let connect = dlsym(network, c"nw_proxy_config_create_http_connect".as_ptr());
            (!host.is_null() && !connect.is_null()).then_some((host as usize, connect as usize))
        }
    }))?;
    // SAFETY: the framework's functions, with the signatures its headers give.
    unsafe {
        Some((
            std::mem::transmute::<usize, CreateHost>(host),
            std::mem::transmute::<usize, CreateConnect>(connect),
        ))
    }
}

/// Makes every connection of `store`'s session go through the proxy on
/// `port` of this machine.
pub(super) fn through_proxy(store: &WKWebsiteDataStore, port: u16) -> Result<(), String> {
    let unsupported = || "web pages are made into PDFs on macOS 14 or later".to_owned();
    // SAFETY: asking an object whether it answers a selector.
    let settable: bool =
        unsafe { msg_send![store, respondsToSelector: sel!(setProxyConfigurations:)] };
    if !settable {
        return Err(unsupported());
    }
    let (create_host, create_connect) = functions().ok_or_else(unsupported)?;
    let port = CString::new(port.to_string()).map_err(|_| unsupported())?;
    // SAFETY: C strings that outlive the call; it returns a +1 endpoint.
    let endpoint = unsafe { Retained::from_raw(create_host(c"127.0.0.1".as_ptr(), port.as_ptr())) }
        .ok_or("the PDF proxy has no endpoint")?;
    // SAFETY: the endpoint above and no TLS options (the proxy is plain
    // TCP on this machine); it returns a +1 configuration.
    let config = unsafe {
        Retained::from_raw(create_connect(
            Retained::as_ptr(&endpoint).cast_mut(),
            std::ptr::null_mut(),
        ))
    }
    .ok_or("the PDF proxy has no configuration")?;
    // SAFETY: an array of the one configuration, given to the setter
    // `respondsToSelector:` found above, which copies it.
    unsafe {
        let configurations: Retained<AnyObject> =
            msg_send![class!(NSArray), arrayWithObject: &*config];
        let () = msg_send![store, setProxyConfigurations: &*configurations];
    }
    Ok(())
}
