//! Auto-generated from official WHATWG HTML §13.5 entities.json (2,231 entries).
//!
//! The generated table is split across the `part_*` submodules below to keep
//! this file within the repository size limit (ARCH §4). [`assemble`] stitches
//! the chunks back together in the exact original entities.json order; the
//! bytes and ordering of every entry are unchanged.

mod part_a;
mod part_b;
mod part_c;
mod part_d;
mod part_e;

/// One named-character-reference entry: the entity name and its code points
/// (one or two, per WHATWG HTML §13.5).
type Entry = (&'static str, &'static [u32]);

const LEN_A: usize = 450;
const LEN_B: usize = 450;
const LEN_C: usize = 450;
const LEN_D: usize = 450;
const LEN_E: usize = 431;
const TOTAL: usize = LEN_A + LEN_B + LEN_C + LEN_D + LEN_E;

/// Concatenate the generated chunks into one flat array, preserving order.
///
/// Runs entirely during const evaluation, so this has no runtime cost.
const fn assemble(
    a: [Entry; LEN_A],
    b: [Entry; LEN_B],
    c: [Entry; LEN_C],
    d: [Entry; LEN_D],
    e: [Entry; LEN_E],
) -> [Entry; TOTAL] {
    let mut out: [Entry; TOTAL] = [("", &[]); TOTAL];
    let mut i = 0;
    let mut k = 0;
    while k < LEN_A {
        out[i] = a[k];
        i += 1;
        k += 1;
    }
    k = 0;
    while k < LEN_B {
        out[i] = b[k];
        i += 1;
        k += 1;
    }
    k = 0;
    while k < LEN_C {
        out[i] = c[k];
        i += 1;
        k += 1;
    }
    k = 0;
    while k < LEN_D {
        out[i] = d[k];
        i += 1;
        k += 1;
    }
    k = 0;
    while k < LEN_E {
        out[i] = e[k];
        i += 1;
        k += 1;
    }
    out
}

/// The full WHATWG named character reference table, in entities.json order.
pub const NAMED_ENTITIES: &[(&str, &[u32])] = &assemble(
    part_a::ENTITIES,
    part_b::ENTITIES,
    part_c::ENTITIES,
    part_d::ENTITIES,
    part_e::ENTITIES,
);
