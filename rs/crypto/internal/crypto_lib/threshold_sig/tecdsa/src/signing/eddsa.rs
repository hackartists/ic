use crate::*;
use ic_types::Randomness;

/// Compute the Fiat-Shamir challenge
///
/// Schnorr signatures are effectively a Sigma protocol proving
/// knowledge of discrete logarithm, made non-interactive using
/// the Fiat-Shamir heuristic; the interactive random challenge
/// is replaced by a random oracle applied to the transcript
/// so far.
///
/// See <https://www.zkdocs.com/docs/zkdocs/zero-knowledge-protocols/schnorr/>
fn ed25519_challenge_hash(
    r: &EccPoint,
    p: &EccPoint,
    msg: &[u8],
) -> ThresholdEcdsaResult<EccScalar> {
    let mut sha512 = ic_crypto_sha2::Sha512::new();
    sha512.write(&r.serialize());
    sha512.write(&p.serialize());
    sha512.write(msg);
    let mut e = sha512.finish();

    // EdDSA interprets the SHA-512 output as little endian,
    // but EccScalar::from_bytes_wide uses big endian
    e.reverse();

    EccScalar::from_bytes_wide(EccCurveType::Ed25519, &e)
}

/// Presignature rerandomization
///
/// Malicious nodes can cause biases in the presignature R transcript
/// due to the use of unblinded commitments in the RandomUnmasked case.
/// We prevent this from being an issue by rerandomizing the R value
/// using information that is not available until the point the signature
/// is created.
///
/// This does not match normal EdDSA signatures, which are deterministic.
///
/// The rerandomization process includes also the step for deriving the subkey
/// that is used for this particular caller (based on derivation path, which
/// includes the canister id). This is because we use the derived key as one
/// of the inputs to the presignature rerandomization step.
///
/// For more information about rerandomization of Schnorr presignatures see
/// "The many faces of Schnorr", Victor Shoup <https://eprint.iacr.org/2023/1019>
struct RerandomizedPresignature {
    /// The derived public key
    derived_key: EccPoint,
    /// The discrete log of the difference between the derived public key
    /// and the master public key
    key_tweak: EccScalar,
    /// The rerandomized presignature commitment
    randomized_pre_sig: EccPoint,
    /// The discrete log of the difference between the rerandomized presignature
    /// and the presignature transcript generated by the IDKG
    presig_randomizer: EccScalar,
}

impl RerandomizedPresignature {
    fn compute(
        message: &[u8],
        randomness: &Randomness,
        derivation_path: &DerivationPath,
        key_transcript: &IDkgTranscriptInternal,
        presig_transcript: &IDkgTranscriptInternal,
    ) -> ThresholdEcdsaResult<Self> {
        let pre_sig = match &presig_transcript.combined_commitment {
            // random unmasked case
            // unlike for ECDSA we require the Schnorr R be generated by random unmasked only
            CombinedCommitment::BySummation(PolynomialCommitment::Simple(c)) => c.constant_term(),
            _ => return Err(ThresholdEcdsaError::UnexpectedCommitmentType),
        };

        let curve = pre_sig.curve_type();

        // EdDSA is only defined for Ed25519
        if curve != EccCurveType::Ed25519 {
            return Err(ThresholdEcdsaError::UnexpectedCommitmentType);
        }

        let idkg_key = key_transcript.constant_term();

        let (key_tweak, _chain_key) = derivation_path.derive_tweak(&idkg_key)?;

        // Rerandomize presignature
        let mut ro = RandomOracle::new("ic-crypto-eddsa-rerandomize-presig");
        ro.add_bytestring("randomness", &randomness.get())?;
        ro.add_bytestring("message", message)?;
        ro.add_point("pre_sig", &pre_sig)?;
        ro.add_point("key_transcript", &idkg_key)?;
        ro.add_scalar("key_tweak", &key_tweak)?;
        let presig_randomizer = ro.output_scalar(curve)?;

        let randomized_pre_sig =
            pre_sig.add_points(&EccPoint::generator_g(curve).scalar_mul(&presig_randomizer)?)?;
        let derived_key =
            idkg_key.add_points(&EccPoint::generator_g(curve).scalar_mul(&key_tweak)?)?;

        Ok(Self {
            derived_key,
            key_tweak,
            randomized_pre_sig,
            presig_randomizer,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdEd25519SignatureShareInternal {
    s: EccScalar,
}

impl ThresholdEd25519SignatureShareInternal {
    pub fn new(
        derivation_path: &DerivationPath,
        message: &[u8],
        randomness: Randomness,
        key_transcript: &IDkgTranscriptInternal,
        key_opening: &CommitmentOpening,
        presig_transcript: &IDkgTranscriptInternal,
        presig_opening: &CommitmentOpening,
    ) -> ThresholdEcdsaResult<Self> {
        let rerandomized = RerandomizedPresignature::compute(
            message,
            &randomness,
            derivation_path,
            key_transcript,
            presig_transcript,
        )?;

        let key_opening = match key_opening {
            CommitmentOpening::Simple(s) => s,
            _ => return Err(ThresholdEcdsaError::UnexpectedCommitmentType),
        };

        let presig_opening = match presig_opening {
            CommitmentOpening::Simple(s) => s,
            _ => return Err(ThresholdEcdsaError::UnexpectedCommitmentType),
        };

        let e = ed25519_challenge_hash(
            &rerandomized.randomized_pre_sig,
            &rerandomized.derived_key,
            message,
        )?;

        let tweaked_x = key_opening.add(&rerandomized.key_tweak)?;

        let xh = tweaked_x.mul(&e)?;

        let r_plus_randomizer = presig_opening.add(&rerandomized.presig_randomizer)?;

        let share = xh.add(&r_plus_randomizer)?;

        Ok(Self { s: share })
    }

    /// Verify a Schnorr signature share
    ///
    /// Schnorr signature shares are quite simple in that they are (ignoring
    /// rerandomization and even-y issues) simply [s] = [k]*e + [r]
    /// where [k] is the key share, [r] is the share of the presignature, and e
    /// is the challenge (which is known to all parties).
    ///
    /// The important thing to note here is that this expression itself gives a
    /// Schnorr signature, namely a signature of e with respect to the node's
    /// share of the key and presignature.  Since the public commitments to
    /// these shares are unblinded, it is possible for us to compute the public
    /// key and presignature associated with the node's shares by evaluating the
    /// respective commmitments at the signer's index
    pub fn verify(
        &self,
        derivation_path: &DerivationPath,
        message: &[u8],
        randomness: Randomness,
        signer_index: NodeIndex,
        key_transcript: &IDkgTranscriptInternal,
        presig_transcript: &IDkgTranscriptInternal,
    ) -> ThresholdEcdsaResult<()> {
        let rerandomized = RerandomizedPresignature::compute(
            message,
            &randomness,
            derivation_path,
            key_transcript,
            presig_transcript,
        )?;

        let e = ed25519_challenge_hash(
            &rerandomized.randomized_pre_sig,
            &rerandomized.derived_key,
            message,
        )?;

        let node_pk = key_transcript
            .combined_commitment
            .commitment()
            .evaluate_at(signer_index)?
            .add_points(&EccPoint::mul_by_g(&rerandomized.key_tweak))?;
        let node_r = presig_transcript
            .combined_commitment
            .commitment()
            .evaluate_at(signer_index)?
            .add_points(&EccPoint::mul_by_g(&rerandomized.presig_randomizer))?;

        let lhs = EccPoint::mul_by_g(&self.s);
        let hp = node_pk.scalar_mul(&e)?;
        let rhs = node_r.add_points(&hp)?;

        if rhs == lhs {
            Ok(())
        } else {
            Err(ThresholdEcdsaError::InvalidSignatureShare)
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        self.s.serialize_tagged()
    }

    pub fn deserialize(raw: &[u8]) -> ThresholdEcdsaSerializationResult<Self> {
        let s = EccScalar::deserialize_tagged(raw)?;

        if s.curve_type() != EccCurveType::Ed25519 {
            return Err(ThresholdEcdsaSerializationError(format!(
                "Unexpected curve for signature share: got {} expected Ed25519",
                s.curve_type()
            )));
        }

        Ok(Self { s })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdEd25519CombinedSignatureInternal {
    r: EccPoint,
    s: EccScalar,
}

impl ThresholdEd25519CombinedSignatureInternal {
    pub fn serialize(&self) -> Vec<u8> {
        let mut v = vec![];
        v.extend_from_slice(&self.r.serialize());
        v.extend_from_slice(&self.s.serialize());
        v
    }

    pub fn deserialize(
        bytes: &[u8],
    ) -> Result<Self, ThresholdEd25519SignatureShareInternalSerializationError> {
        const ED25519: EccCurveType = EccCurveType::Ed25519;
        const EXPECTED_LEN: usize = ED25519.scalar_bytes() + ED25519.point_bytes();

        if bytes.len() != EXPECTED_LEN {
            return Err(ThresholdEd25519SignatureShareInternalSerializationError(
                format!(
                    "Bad signature length, expected {EXPECTED_LEN} but got {}",
                    bytes.len()
                ),
            ));
        }

        let (point_bytes, scalar_bytes) = bytes.split_at(ED25519.point_bytes());

        let r = EccPoint::deserialize(ED25519, point_bytes).map_err(|e| {
            ThresholdEd25519SignatureShareInternalSerializationError(format!("Invalid r: {:?}", e))
        })?;

        let s = EccScalar::deserialize(ED25519, scalar_bytes).map_err(|e| {
            ThresholdEd25519SignatureShareInternalSerializationError(format!("Invalid s: {:?}", e))
        })?;

        Ok(Self { r, s })
    }

    /// Combine shares into a Ed25519 signature
    pub fn new(
        derivation_path: &DerivationPath,
        message: &[u8],
        randomness: Randomness,
        key_transcript: &IDkgTranscriptInternal,
        presig_transcript: &IDkgTranscriptInternal,
        reconstruction_threshold: NumberOfNodes,
        sig_shares: &BTreeMap<NodeIndex, ThresholdEd25519SignatureShareInternal>,
    ) -> ThresholdEcdsaResult<Self> {
        let reconstruction_threshold = reconstruction_threshold.get() as usize;
        if sig_shares.len() < reconstruction_threshold {
            return Err(ThresholdEcdsaError::InsufficientDealings);
        }

        let rerandomized = RerandomizedPresignature::compute(
            message,
            &randomness,
            derivation_path,
            key_transcript,
            presig_transcript,
        )?;

        let mut x_values = Vec::with_capacity(reconstruction_threshold);
        let mut samples = Vec::with_capacity(reconstruction_threshold);

        for (index, sig_share) in sig_shares.iter().take(reconstruction_threshold) {
            x_values.push(*index);
            samples.push(sig_share.s.clone());
        }

        let coefficients = LagrangeCoefficients::at_zero(EccCurveType::Ed25519, &x_values)?;
        let combined_s = coefficients.interpolate_scalar(&samples)?;

        Ok(Self {
            r: rerandomized.randomized_pre_sig,
            s: combined_s,
        })
    }

    /// Verify a ED25519 Schnorr signature
    ///
    /// In addition to normal signature verification, this also checks
    /// that the signature was generated using a specific presignature
    /// transcript
    pub fn verify(
        &self,
        derivation_path: &DerivationPath,
        message: &[u8],
        randomness: Randomness,
        presig_transcript: &IDkgTranscriptInternal,
        key_transcript: &IDkgTranscriptInternal,
    ) -> ThresholdEcdsaResult<()> {
        if self.r.is_infinity()? || self.s.is_zero() {
            return Err(ThresholdEcdsaError::InvalidSignature);
        }

        let rerandomized = RerandomizedPresignature::compute(
            message,
            &randomness,
            derivation_path,
            key_transcript,
            presig_transcript,
        )?;

        if self.r != rerandomized.randomized_pre_sig {
            return Err(ThresholdEcdsaError::InvalidSignature);
        }

        let e = ed25519_challenge_hash(
            &rerandomized.randomized_pre_sig,
            &rerandomized.derived_key,
            message,
        )?;

        // R = s*G - e*P
        let g = EccPoint::generator_g(EccCurveType::Ed25519);
        let rp = EccPoint::mul_2_points(&g, &self.s, &rerandomized.derived_key, &e.negate())?;

        // We already checked above that self.r is not infinity and has even y:
        if rp != self.r {
            return Err(ThresholdEcdsaError::InvalidSignature);
        }

        // accept:
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThresholdEd25519SignatureShareInternalSerializationError(pub String);
