plugins {
    application
}

dependencies {
    implementation(project(":common"))
}

application {
    mainClass.set("net.knego.hiperf.threadhandoff.channel.Main")
}
