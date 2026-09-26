use ferry::protocol::{IdentityBody, Packet, PacketCodec, PairingBody};
use serde_json::{Value, json};

const IDENTITY: &str = include_str!("fixtures/identity.json");
const PAIRING_REQUEST: &str = include_str!("fixtures/pairing_request.json");
const PAYLOAD: &str = include_str!("fixtures/payload.json");

fn decode_fixture(fixture: &str) -> Packet {
    let mut codec = PacketCodec::new(4096);
    codec.decode(fixture.as_bytes()).unwrap().remove(0)
}

#[test]
fn identity_fixture_round_trips_and_validates() {
    let packet = decode_fixture(IDENTITY);
    let body: IdentityBody = packet.body_as().unwrap();

    body.validate().unwrap();
    let encoded = PacketCodec::new(4096).encode(&packet).unwrap();
    let decoded = PacketCodec::new(4096).decode(&encoded).unwrap();
    assert_eq!(decoded, vec![packet]);
}

#[test]
fn pairing_request_fixture_round_trips() {
    let packet = decode_fixture(PAIRING_REQUEST);
    let body: PairingBody = packet.body_as().unwrap();

    assert!(body.pair);
    assert_eq!(body.timestamp, Some(1_758_510_000));
    let rebuilt = Packet::from_body(packet.id.clone(), packet.packet_type.clone(), &body).unwrap();
    assert_eq!(rebuilt.body, packet.body);
}

#[test]
fn payload_envelope_uses_exact_wire_names() {
    let packet = decode_fixture(PAYLOAD);
    let value = serde_json::to_value(&packet).unwrap();

    assert_eq!(value["payloadSize"], 882);
    assert_eq!(value["payloadTransferInfo"]["port"], 1739);
    assert!(value.get("payload_size").is_none());
}

#[test]
fn unknown_packet_and_fields_survive_a_round_trip() {
    let original = json!({
        "id": 123,
        "type": "kdeconnect.future.feature",
        "body": {"knownNowhere": true, "nested": {"value": 4}},
        "futureEnvelopeField": "preserve me"
    });
    let mut wire = serde_json::to_vec(&original).unwrap();
    wire.push(b'\n');

    let packet = PacketCodec::new(4096).decode(&wire).unwrap().remove(0);
    let rebuilt: Value = serde_json::to_value(packet).unwrap();

    assert_eq!(rebuilt, original);
}
