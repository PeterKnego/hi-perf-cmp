// Shared library: the result-contract record + stdout emitter, plus the
// comparability-critical Stats, env-driven Config, and Measure loop reused by
// every network-rtt experiment.
plugins {
    `java-library`
}

dependencies {
    testImplementation("org.junit.jupiter:junit-jupiter:5.11.0")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()
}
