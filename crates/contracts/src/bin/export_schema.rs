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
        "WorkerRequest": schema_for!(witvoice_contracts::worker::WorkerRequest),
        "WorkerResponse": schema_for!(witvoice_contracts::worker::WorkerResponse),
        "worker_media": {
            "engine_id": witvoice_contracts::worker::WORKER_ENGINE_ID,
            "backend_label": witvoice_contracts::worker::WORKER_BACKEND_LABEL,
            "control_max_bytes": witvoice_contracts::CONTROL_MAX_BYTES,
            "header_bytes": witvoice_contracts::worker::WORKER_HEADER_BYTES,
            "maximum_samples": witvoice_contracts::worker::WORKER_MAX_SAMPLES,
            "native_rate": witvoice_contracts::worker::WORKER_INPUT_RATE,
            "chunk_samples": witvoice_contracts::worker::WORKER_CHUNK_SAMPLES,
            "header_format": ">BBHQIIQIII",
            "pcm_format": "mono_f32le",
            "golden_header_hex": witvoice_contracts::worker::WorkerMediaHeader {
                kind: witvoice_contracts::worker::WorkerMediaKind::Source,
                session_tag: 0x0102030405060708, epoch: 9, sequence: 0xffff_fffe,
                source_sample_index: 2560, sample_rate: 16000, sample_count: 2560
            }.encode().expect("valid fixed worker vector").iter().map(|b| format!("{b:02x}")).collect::<String>()
        },
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
