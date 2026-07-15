plugins {
    `java-library`
}

dependencies {
    api(project(":common"))
    // Agrona: primitive collections (Long2ObjectHashMap) + buffers for SBE.
    api("org.agrona:agrona:1.21.0")

    testImplementation("org.junit.jupiter:junit-jupiter:5.11.0")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()
}
