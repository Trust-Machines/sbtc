//! # Canonical encoding and decoding for the sBTC signer
//!
//! The purpose of this module is to define how to encode and decode signer
//! messages as byte sequences.
//!
//! ## Codec specification
//!
//! The signers communicate with each other by sending protobuf messages
//! serialized in a canonical way. Specifically, signer message
//! serialization must adhere to the following constraints:
//! 1. Each field must be serialized in the order of its tag number. If
//!    `field_a` has a lower tag than `field_b`, then `field_a` will be
//!    serialized before `field_b`.
//! 2. Map protobuf fields can only be used if the key type is
//!    well-ordered. In particular, the Rust version of these types must
//!    implement the `Ord` trait.
//! 3. Map elements must be serialized in order of their keys.
//! 4. The specific encoding and decoding of a field or message must follow
//!    the protobuf spec. In particular, missing fields are not serialized.
//!
//! This is achieved by doing the following:
//! 1. Use [`prost`] to generate rust serialization and deserialization
//!    code. We do so in a way that satisfies all four of the above
//!    constraints.
//! 2. Provide a `ProtoSerializable` trait for types that can be serialized
//!    by their corresponding protobuf analog.
//! 3. Provide the `Encode` and `Decode` traits.  Use them for
//!    serialization and deserialization of any types that implement the
//!    `ProtoSerializable` trait.
//!

use std::io;

use prost::Message as _;

use crate::error::Error;

/// Utility trait to specify mapping between internal types and proto
/// counterparts. The implementation of `Encode` and `Decode` for a type
/// `T` implementing `ProtoSerializable` assume `T: Into<Message> +
/// TryFrom<Message>`.
/// ```
/// use signer::codec::ProtoSerializable;
/// use signer::proto;
///
/// struct MyPublicKey(signer::keys::PublicKey);
///
/// impl ProtoSerializable for MyPublicKey {
///     type Message = proto::PublicKey;
///
///     fn type_tag(&self) -> &'static str {
///         "MY_PUBLIC_KEY"
///     }
/// }
/// ```
pub trait ProtoSerializable {
    /// The proto message type used for conversions
    type Message: ::prost::Message + Default;
    /// A message type tag used for hashing the message before signing.
    fn type_tag(&self) -> &'static str;
}

/// Provides a method for encoding an object into a writer using a canonical serialization format.
///
/// This trait is designed to be implemented by types that need to serialize their data into a byte stream
/// in a standardized format, primarily to ensure consistency across different components of the signer system.
pub trait Encode: Sized {
    /// Encodes the calling object into a vector of bytes.
    ///
    /// # Returns
    /// The vector of bytes.
    /// TODO: change to &self
    fn encode_to_vec(self) -> Vec<u8>;
}

/// Provides a method for decoding an object from a reader using a canonical deserialization format.
///
/// This trait is intended for types that need to reconstruct themselves from a byte stream, ensuring
/// that objects across the signer system are restored from bytes uniformly.
///
/// It includes a generic method for reading from any input that implements `io::Read`, as well as
/// a convenience method for decoding from a byte slice.
pub trait Decode: Sized {
    /// Decodes an object from a reader in a canonical format.
    ///
    /// # Arguments
    /// * `reader` - An object implementing `io::Read` from which the bytes will be read.
    ///
    /// # Returns
    /// A `Result` which is `Ok` containing the decoded object, or an `Error` if decoding failed.
    fn decode<R: io::Read>(reader: R) -> Result<Self, Error>;
}

impl<T> Encode for T
where
    T: ProtoSerializable + Clone,
    T: Into<<T as ProtoSerializable>::Message>,
{
    fn encode_to_vec(self) -> Vec<u8> {
        let message: <Self as ProtoSerializable>::Message = self.into();
        prost::Message::encode_to_vec(&message)
    }
}

impl<T> Decode for T
where
    T: ProtoSerializable + Clone,
    T: TryFrom<<T as ProtoSerializable>::Message, Error = Error>,
{
    fn decode<R: io::Read>(mut reader: R) -> Result<Self, Error> {
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(CodecError::DecodeIOError)?;

        let message =
            <<T as ProtoSerializable>::Message>::decode(&*buf).map_err(CodecError::DecodeError)?;

        T::try_from(message)
    }
}

/// The error used in the [`Encode`] and [`Decode`] trait.
#[derive(thiserror::Error, Debug)]
pub enum CodecError {
    /// Decode error
    #[error("Decode error: {0}")]
    DecodeError(#[source] prost::DecodeError),
    /// Decode error
    #[error("Decode error: {0}")]
    DecodeIOError(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use fake::Dummy as _;
    use rand::SeedableRng as _;

    use crate::keys::PublicKey;
    use crate::proto;

    use super::*;

    impl ProtoSerializable for PublicKey {
        type Message = proto::PublicKey;

        fn type_tag(&self) -> &'static str {
            "SBTC_PUBLIC_KEY"
        }
    }

    #[test]
    fn public_key_should_be_able_to_encode_and_decode_correctly() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(46);
        let message = PublicKey::dummy_with_rng(&fake::Faker, &mut rng);

        let encoded = message.encode_to_vec();

        let decoded = <PublicKey as Decode>::decode(encoded.as_slice()).unwrap();

        assert_eq!(decoded, message);
    }
}
