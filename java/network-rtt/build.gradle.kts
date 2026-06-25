plugins {
    application
}

dependencies {
    implementation(project(":common"))

    testImplementation("org.junit.jupiter:junit-jupiter:5.11.0")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

application {
    mainClass.set("net.knego.hiperf.networkrtt.Main")
}

tasks.test {
    useJUnitPlatform()
}
