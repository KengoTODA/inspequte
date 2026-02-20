plugins {
    `java-gradle-plugin`
    `kotlin-dsl`
    id("com.gradle.plugin-publish") version "2.0.0"
}

group = "io.github.kengotoda.inspequte"
val pluginVersion = providers.gradleProperty("pluginVersion")
    .orElse(providers.fileContents(layout.projectDirectory.file("version.txt")).asText.map { it.trim() })
version = pluginVersion.get()

repositories {
    mavenCentral()
    gradlePluginPortal()
}

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(21))
    }
}

gradlePlugin {
    website = "https://github.com/KengoTODA/inspequte"
    vcsUrl = "https://github.com/KengoTODA/inspequte.git"

    plugins {
        create("inspequtePlugin") {
            id = "io.github.kengotoda.inspequte"
            displayName = "inspequte Gradle Plugin"
            description = "Runs inspequte for each Java source set and emits SARIF reports."
            implementationClass = "io.github.kengotoda.inspequte.gradle.InspequtePlugin"
            tags.set(listOf("inspequte", "sarif", "static-analysis", "jvm"))
        }
    }
}

dependencies {
    testImplementation(platform("org.junit:junit-bom:5.14.3"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
}
