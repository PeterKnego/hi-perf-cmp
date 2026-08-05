// Shared configuration applied to every subproject.
subprojects {
    apply(plugin = "java")

    repositories {
        mavenCentral()
    }

    extensions.configure<JavaPluginExtension> {
        toolchain {
            languageVersion.set(JavaLanguageVersion.of(21))
        }
    }

    // Diagnostic hook: extra JVM args for the benchmark process (not the
    // Gradle JVM) via BENCH_JVM_ARGS, e.g. -Xlog:gc* for the GC-attribution
    // runs. Unset (the normal case) it configures nothing, so grid runs are
    // byte-identical to before this hook existed.
    tasks.withType<JavaExec>().configureEach {
        System.getenv("BENCH_JVM_ARGS")?.trim()?.takeIf { it.isNotEmpty() }?.let {
            jvmArgs(it.split(Regex("\\s+")))
        }
    }
}
