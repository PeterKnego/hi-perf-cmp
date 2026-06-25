plugins {
    application
}

dependencies {
    implementation(project(":common"))

    // Pure-Java QUIC implementation (no native libraries). Provides both a
    // server echo responder and a client over a raw bidirectional stream.
    // Transitive deps: tech.kwik:agent15 (TLS 1.3), at.favre.lib:hkdf,
    // io.whitfin:siphash — all pure Java.
    implementation("tech.kwik:kwik:0.10.10")
}

// The in-memory self-signed cert is generated with the JDK's internal
// sun.security helpers (keeps QUIC dependency-free of BouncyCastle); these
// require --add-exports at both compile and run time.
val certExports = listOf(
    "--add-exports", "java.base/sun.security.x509=ALL-UNNAMED",
    "--add-exports", "java.base/sun.security.tools.keytool=ALL-UNNAMED",
)

tasks.withType<JavaCompile>().configureEach {
    options.compilerArgs.addAll(certExports)
}

application {
    mainClass.set("net.knego.hiperf.networkrtt.quic.Main")
    applicationDefaultJvmArgs = certExports
}
