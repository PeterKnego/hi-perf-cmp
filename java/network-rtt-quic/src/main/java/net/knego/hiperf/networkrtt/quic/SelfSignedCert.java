package net.knego.hiperf.networkrtt.quic;

import java.security.KeyStore;
import java.security.PrivateKey;
import java.security.cert.X509Certificate;
import sun.security.tools.keytool.CertAndKeyGen;
import sun.security.x509.X500Name;

/**
 * Generates an in-memory self-signed certificate + key for the QUIC server.
 *
 * <p>For a loopback/private-network latency benchmark we do not need a real CA:
 * the server presents a freshly generated self-signed cert and the client skips
 * verification. Using the JDK's internal {@code CertAndKeyGen} keeps this
 * dependency-free (no BouncyCastle); the required {@code --add-exports} JVM
 * flags are set in {@code build.gradle.kts}.
 */
final class SelfSignedCert {

    /** KeyStore alias and (throwaway) password under which the key is stored. */
    static final String ALIAS = "hperf-rtt";
    static final char[] PASSWORD = "hperf-rtt".toCharArray();

    private SelfSignedCert() {}

    /**
     * Build a single-entry {@link KeyStore} (type PKCS12) holding a fresh
     * self-signed RSA cert + private key for {@code CN=localhost}, valid for
     * one day.
     */
    static KeyStore generate() throws Exception {
        CertAndKeyGen gen = new CertAndKeyGen("RSA", "SHA256withRSA");
        gen.generate(2048);
        PrivateKey key = gen.getPrivateKey();
        X509Certificate cert = gen.getSelfCertificate(
                new X500Name("CN=localhost"), 24 * 60 * 60);

        KeyStore ks = KeyStore.getInstance("PKCS12");
        ks.load(null, null);
        ks.setKeyEntry(ALIAS, key, PASSWORD, new X509Certificate[] {cert});
        return ks;
    }
}
