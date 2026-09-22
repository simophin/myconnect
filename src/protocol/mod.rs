//! KDE Connect wire packet models and bounded framing.

mod codec;
mod packet;
mod verification;

pub use codec::{CodecError, PacketCodec};
pub use packet::{
    BodyError, DeviceType, IdentityBody, IdentityValidationError, Packet, PairingBody,
};
pub use verification::verification_code;
