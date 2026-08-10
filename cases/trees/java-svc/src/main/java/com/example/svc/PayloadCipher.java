package com.example.svc;

import javax.crypto.Cipher;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.SecretKeySpec;

// The `weak-cipher`/`ecb-mode` arms of `security/weak-cipher` — their own rule since the 2026-08-09
// split (until then they lived inside `security/weak-crypto`, which now carries only the hash half on
// the parser's call-site channel; EXPECTED anchors :19 and :27 report under `security/weak-cipher`).
// Until this file the two arms had unit tests and nothing else: the corpus contained no
// `Cipher.getInstance` line, so they shipped unscored — a rule arm no fixture reaches is not
// passing; it is unmeasured, and it can rot without any run going red.
// Java is lexically parsed here, so this need not compile. Three transforms, one per verdict.
public class PayloadCipher {

  // ecb-mode — must fire. ECB encrypts each block independently, so two identical plaintext blocks
  // produce two identical ciphertext blocks and the shape of the payload survives encryption.
  public byte[] sealLegacyExport(byte[] key, byte[] payload) throws Exception {
    Cipher cipher = Cipher.getInstance("AES/ECB/PKCS5Padding");
    cipher.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "AES"));
    return cipher.doFinal(payload);
  }

  // weak-cipher — must fire. Single DES has 56 effective key bits and is brute-forceable with
  // commodity hardware; the transform name carries no mode, so the provider default applies as well.
  public byte[] sealPartnerHandoff(byte[] key, byte[] payload) throws Exception {
    Cipher cipher = Cipher.getInstance("DES");
    cipher.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "DES"));
    return cipher.doFinal(payload);
  }

  // NEGATIVE CONTROL — must NOT fire. Authenticated encryption with a per-message nonce: neither the
  // weak-cipher arm (no DES/RC4/RC2) nor the ecb-mode arm (no `/ECB/` segment) may claim it.
  public byte[] seal(byte[] key, byte[] nonce, byte[] payload) throws Exception {
    Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
    cipher.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "AES"), new GCMParameterSpec(128, nonce));
    return cipher.doFinal(payload);
  }
}
