//! Runtime functions callable from Core as `Atom::Builtin(name)`.
//!
//! Every entry is an export of `compiler/runtime/js/sakura_runtime.js` with the same name. `Direct`
//! builtins are plain JS functions `f(args…) -> value` (they may throw a defect); `Cps` builtins
//! take callbacks and follow the CPS ABI `f(args…, ctx, k) -> step` (see `runtime/js/ABI.md`).
//! A conformance test checks this table against the runtime's exports and arities.
//!
//! Two columns feed the ownership pass (design §17.149, [`crate::opt::owned`]): `fresh` marks a
//! result that shares no node with any input or other live value (`[fresh]`), and `consumes`
//! the argument an owned-capable builtin takes over (`[consumes 0]`). Each owned-capable
//! builtin `f` has an in-place twin `f_owned` in the runtime.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abi {
    Direct,
    Cps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Builtin {
    pub name: &'static str,
    /// Sakura Lang-level arity (excluding `ctx`, `k`).
    pub arity: usize,
    pub abi: Abi,
    /// The result shares no node with an input or any other value, so it is an owned origin.
    /// False for anything that can return an input or a shared value (`list_slice`,
    /// `list_concat`) and for the empty singletons, which the ownership pass treats on their own.
    pub fresh: bool,
    /// For an owned-capable builtin, the argument position it takes over: when that argument is
    /// owned, the runtime's `<name>_owned` updates it in place.
    pub consumes: Option<usize>,
}

const PLAIN: Builtin = Builtin {
    name: "",
    arity: 0,
    abi: Abi::Direct,
    fresh: false,
    consumes: None,
};

macro_rules! cols {
    () => {
        PLAIN
    };
    (fresh) => {
        Builtin {
            fresh: true,
            ..PLAIN
        }
    };
    (consumes $p:literal) => {
        Builtin {
            consumes: Some($p),
            ..PLAIN
        }
    };
}

macro_rules! table {
    ($($abi:ident $name:ident / $arity:literal $([$($col:tt)*])?),* $(,)?) => {
        pub const BUILTINS: &[Builtin] = &[
            $(Builtin {
                name: stringify!($name),
                arity: $arity,
                abi: Abi::$abi,
                ..cols!($($($col)*)?)
            }),*
        ];
    };
}

table! {
    // List (persistent, design §13.4)
    Direct list_empty/0, Direct list_len/1, Direct list_get/2, Direct list_at/2,
    Direct list_push/2 [consumes 0], Direct list_set/3 [consumes 0], Direct list_concat/2,
    Direct list_range/2 [fresh], Direct list_reverse/1,
    Direct list_join/2, Direct list_sum/1, Direct list_first/1, Direct list_last/1,
    Direct list_contains/2, Direct list_slice/3, Direct list_sort/1, Direct list_zip/2,
    Direct list_enumerate/1, Direct list_unique/1,
    // Transient builders, used only inside std/list bodies (design §17.116)
    Direct list_builder/0, Direct list_builder_push/2, Direct list_builder_finish/1 [fresh],
    Cps list_map/2, Cps list_filter/2, Cps list_fold/3, Cps list_find/2, Cps list_all/2,
    Cps list_any/2, Cps list_flat_map/2, Cps list_sort_by/2,
    // Map / Set (insertion-ordered)
    Direct map_empty/0, Direct map_of/1, Direct map_get/2, Direct map_insert/3 [consumes 0],
    Direct map_remove/2 [consumes 0],
    Direct map_contains/2, Direct map_entries/1, Direct map_keys/1, Direct map_values/1,
    Direct map_len/1, Cps map_update/3,
    Direct set_empty/0, Direct set_of/1, Direct set_insert/2 [consumes 0], Direct set_contains/2,
    Direct set_to_list/1,
    // String
    Direct string_len/1, Direct string_chars/1 [fresh], Direct string_graphemes/1 [fresh],
    Direct string_split/2 [fresh],
    Direct string_trim/1, Direct string_starts_with/2, Direct string_ends_with/2,
    Direct string_contains/2, Direct string_replace/3, Direct string_to_lower/1,
    Direct string_to_upper/1, Direct string_parse_int/1, Direct string_parse_float/1,
    Direct string_repeat/2, Direct string_pad_left/3, Direct string_slice/3,
    Direct string_to_bytes/1,
    // Int / Float
    Direct int_to_float/1, Direct int_to_string/1, Direct int_abs/1, Direct int_min/2,
    Direct int_max/2, Direct int_clamp/3, Direct int_pow/2, Direct int_checked_add/2,
    Direct int_wrapping_add/2,
    Direct float_round/1, Direct float_to_int_checked/1, Direct float_sqrt/1,
    Direct float_is_nan/1, Direct float_format/2,
    // Option
    Direct option_expect/2,
    // `get` + an immediately consumed `option_expect`, fused by `opt` (design §17.151)
    Direct list_get_expect/3, Direct map_get_expect/3,
    // `map_get` + an immediately consumed `Option.unwrap_or`, fused by `opt` (design §17.275 (4))
    Direct map_get_or/3,
    // Equality that only constructor tags decide, rewritten by `opt` (design §17.183)
    Direct tag_eq/2,
    // Host primitives behind the std effects (design §10.2) and deterministic generators
    Direct console_host/0, Direct console_print/1, Direct console_read_line/0,
    Direct clock_now_ms/0, Direct clock_monotonic_ms/0, Cps clock_sleep_ms/1,
    Cps clock_virtual_sleep/2,
    Direct random_int/2, Direct random_float/0, Direct random_bytes/1, Direct random_uuid/0,
    Direct env_get/1, Direct log_write/1,
    Direct clock_host/0, Direct random_host/0, Direct log_stderr/0, Direct log_json/0,
    // Show (dynamic; calls with a known type lower to `PrimOp::Show`)
    Direct show/1,
    // Bytes
    Direct bytes_len/1, Direct bytes_get/2, Direct bytes_from_list/1, Direct string_from_bytes/1,
    Direct set_len/1,
    // Bytes, Base64 and text encodings (RFC 0002 §2.1)
    Direct bytes_slice/3, Direct bytes_concat/1, Direct bytes_to_base64/3,
    Direct bytes_from_base64/2, Direct bytes_from_base64_lenient/1, Direct encoding_decode/3,
    Direct encoding_encode/2,
    // Cryptography (RFC 0002 §2.7)
    Direct crypto_digest/2, Direct crypto_hmac/3, Direct crypto_hmac_verify/4,
    Direct crypto_hkdf/5, Direct crypto_pbkdf2/5,
    // Tasks and tests
    Cps defect_catch/1, Cps test_expect_defect/1, Cps property_run/8, Direct test_env/2,
    // Secret, Json
    Direct secret_wrap/1, Direct secret_expose/1, Direct json_parse/1, Direct json_encode/2,
    Direct json_decode/2, Direct json_schema/2, Direct untrusted_of/1, Direct untrusted_value/1,
    // Fs, Cli
    Direct fs_root/0, Direct fs_memory_root/1, Direct fs_subdir/2, Direct fs_read/2,
    Direct fs_write/3, Direct fs_list/2, Direct fs_exists/2, Direct cli_args/0, Direct cli_exit/1,
    Cps cli_parse/2, Direct file_open/3, Cps file_closing/2, Direct file_read/2,
    Direct file_write/2,
    // Task
    Direct task_host/0, Direct task_deterministic/1,
    // Http
    Cps http_send/1, Direct http_recording_open/2, Direct http_recording_lookup/2,
    Direct http_recording_append/3, Cps http_stream_open/1, Cps http_stream_closing/2,
    Cps http_stream_read/2, Direct http_body_of/1, Direct http_recording_lookup_stream/2,
    Direct http_recording_append_stream/5,
    // Sql
    Direct sql_memory/0, Direct sql_sqlite/1,
    // Llm
    Direct llm_anthropic/2,
    // Serve
    Cps serve_listen/2,
    // WebSocket (RFC 0002 §1, plan `web-apis.md` phase 8)
    Cps ws_connect/2, Cps ws_send/2, Cps ws_receive/1, Direct ws_close/3,
    // Compiler support
    Direct match_failure/0, Direct contract_enter/0, Direct contract_exit/0,
}

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}
