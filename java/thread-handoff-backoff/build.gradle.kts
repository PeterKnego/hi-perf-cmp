plugins {
    application
}

dependencies {
    implementation(project(":common"))
    // The real Agrona ladder — experiment-specific dependency, artifact-local
    // per house rule (same version as the smr-collections cells).
    implementation("org.agrona:agrona:1.21.0")
}

application {
    mainClass.set("net.knego.hiperf.threadhandoff.backoff.Main")
}
