//! KDE Connect wire packet models and bounded framing.

mod codec;
mod packet;
mod verification;

pub use codec::{CodecError, PacketCodec};
pub(crate) use packet::is_forbidden_name_character;
pub use packet::{
    BodyError, DeviceType, IdentityBody, IdentityValidationError, Packet, PairingBody,
    is_valid_device_name,
};
pub use verification::verification_code;
