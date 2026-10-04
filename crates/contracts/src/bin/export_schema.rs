use schemars::schema_for;
use witvoice_contracts::{control::*, state::SessionState};
fn main() {
    let schemas = serde_json::json!({
        "protocol_version": witvoice_contracts::PROTOCOL_VERSION,
        "control_max_bytes": witvoice_contracts::CONTROL_MAX_BYTES,
        "profile_max_bytes": witvoice_contracts::PROFILE_MAX_BYTES,
        "Request": schema_for!(Request),
        "Response": schema_for!(Response),
        "PeerRequest": schema_for!(PeerRequest),
        "PreparedCapabilities": schema_for!(PreparedCapabilities),
        "SessionState": schema_for!(SessionState),
        "QueueBudget": schema_for!(QueueBudget),
        "golden_media": {
            "kind": 1, "session_tag": "72623859790382856", "epoch": 9,
            "sequence": 4294967294_u32, "media_sample_index": "480", "source_sample_index": "480", "sample_count": 480,
            "header_hex": witvoice_contracts::media::MediaHeader {
                kind: witvoice_contracts::media::MediaKind::Source, session_tag: 0x0102030405060708,
                epoch: 9, sequence: 0xffff_fffe, media_sample_index: 480, source_sample_index: 480, sample_count: 480
            }.encode().expect("fixed valid vector").iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "pcm_sample_hex": "0080ff7f"
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&schemas).expect("fixed schemas serialize")
    );
}
