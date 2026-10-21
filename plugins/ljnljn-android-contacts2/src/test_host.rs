//! 测试支撑：一个最小“假宿主”，通过 SDK 公开的 C ABI v1 驱动真实的
//! `FileAnalyzer::analyze`，验证 `File::materialize()` → 只读 SQLite → LPP 结果
//! 的整条链路。
//!
//! SDK 的 `File` 只有 `pub(crate)` 构造器，插件无法直接造一个；这里和官方宿主
//! 一样从 `lovelycarve_plugin_v1` 入口创建实例，用一份临时数据库路径应答
//! `file.materialize`，不依赖 LovelyCarve 程序，也不使用真实检材。

use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use lovelycarve_plugin::abi::{self, Buffer, Bytes, HostApiV1, PluginApiV1, HOST_API_SIZE};
use lovelycarve_plugin::{value, Value};

extern "C" {
    fn lovelycarve_plugin_v1(host: *const HostApiV1, plugin: *mut PluginApiV1) -> i32;
}

fn fixture() -> &'static Mutex<Option<std::path::PathBuf>> {
    static FIXTURE: OnceLock<Mutex<Option<std::path::PathBuf>>> = OnceLock::new();
    FIXTURE.get_or_init(|| Mutex::new(None))
}

fn materialize_calls() -> &'static AtomicUsize {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    &CALLS
}

fn host_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

unsafe extern "C" fn fake_call(
    _context: *mut c_void,
    method: Bytes,
    _params: Bytes,
    out: *mut Buffer,
) -> i32 {
    let method = String::from_utf8_lossy(unsafe { method.as_slice() }).to_string();
    let (status, reply) = match method.as_str() {
        "file.materialize" => {
            materialize_calls().fetch_add(1, Ordering::SeqCst);
            let path = fixture()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
                .expect("fake host fixture path");
            let size = std::fs::metadata(&path).expect("fixture metadata").len();
            let path_text = path.to_string_lossy().into_owned();
            (
                abi::STATUS_OK,
                value::map([
                    ("lease_id", Value::from("lease-1")),
                    ("path", Value::from(path_text)),
                    ("size", Value::from(size)),
                ]),
            )
        }
        "file.release" | "host.progress" => (abi::STATUS_OK, Value::Nil),
        _ => (
            abi::STATUS_ERROR,
            value::map([
                ("code", Value::from("METHOD_NOT_FOUND")),
                ("message", Value::from(method.as_str())),
            ]),
        ),
    };
    let encoded = value::encode(&reply).expect("encode fake reply");
    unsafe {
        *out = Buffer::from_vec(encoded);
    }
    status
}

unsafe extern "C" fn fake_free_buffer(_context: *mut c_void, buffer: *mut Buffer) {
    unsafe {
        (*buffer).free_vec();
    }
}

unsafe extern "C" fn fake_log(_context: *mut c_void, _level: u32, _message: Bytes) {}

// 占位表项：入口会写入真实函数表，这里只满足 Rust 的初始化要求。
unsafe extern "C" fn describe_stub(_instance: *mut c_void, _out: *mut Buffer) -> i32 {
    abi::STATUS_ERROR
}

unsafe extern "C" fn call_stub(
    _instance: *mut c_void,
    _call_id: u64,
    _method: Bytes,
    _params: Bytes,
    _out: *mut Buffer,
) -> i32 {
    abi::STATUS_ERROR
}

unsafe extern "C" fn cancel_stub(_instance: *mut c_void, _call_id: u64) {}

unsafe extern "C" fn free_stub(_instance: *mut c_void, buffer: *mut Buffer) {
    unsafe {
        (*buffer).free_vec();
    }
}

unsafe extern "C" fn destroy_stub(_instance: *mut c_void) {}

/// 走一次完整的 `analyzer.analyze`，返回解码后的 LPP 结果值。
pub fn analyze_via_host(path: &Path) -> Value {
    let _guard = host_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *fixture()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path.to_path_buf());
    materialize_calls().store(0, Ordering::SeqCst);

    let mut plugin = PluginApiV1 {
        abi_version: abi::ABI_VERSION,
        struct_size: abi::PLUGIN_API_SIZE,
        instance: std::ptr::null_mut(),
        describe: describe_stub,
        call: call_stub,
        cancel: cancel_stub,
        free_buffer: free_stub,
        destroy: destroy_stub,
    };
    let host = HostApiV1 {
        abi_version: abi::ABI_VERSION,
        struct_size: HOST_API_SIZE,
        context: std::ptr::null_mut(),
        call: fake_call,
        free_buffer: fake_free_buffer,
        log: fake_log,
    };
    let status = unsafe { lovelycarve_plugin_v1(&host, &mut plugin) };
    assert_eq!(status, abi::STATUS_OK, "插件入口失败");

    let size = std::fs::metadata(path).expect("fixture metadata").len();
    let handle = value::map([
        ("file_id", Value::from("fixture-file")),
        ("name", Value::from("contacts2.db")),
        ("size", Value::from(size)),
        ("mime", Value::from("application/vnd.sqlite3")),
    ]);
    let params = value::map([
        ("analyzer", Value::from(crate::ANALYZER_FILE)),
        ("file", handle),
    ]);
    let encoded = value::encode(&params).expect("encode analyze params");

    let mut out = Buffer::empty();
    let status = unsafe {
        (plugin.call)(
            plugin.instance,
            7,
            Bytes::from_slice(b"analyzer.analyze"),
            Bytes::from_slice(&encoded),
            &mut out,
        )
    };
    let bytes = unsafe { out.as_slice().to_vec() };
    unsafe {
        (plugin.free_buffer)(plugin.instance, &mut out);
        (plugin.destroy)(plugin.instance);
    }
    *fixture()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;

    assert_eq!(status, abi::STATUS_OK, "analyzer.analyze 失败");
    assert!(
        materialize_calls().load(Ordering::SeqCst) >= 1,
        "分析器必须通过 File::materialize() 取得路径"
    );
    value::decode(&bytes).expect("decode analyze result")
}
