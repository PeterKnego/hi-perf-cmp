plugins {
    application
}

dependencies {
    implementation(project(":common"))
    implementation(project(":smr-collections-common"))
}

application {
    mainClass.set("net.knego.hiperf.smrcollections.snapshot.Main")
}
