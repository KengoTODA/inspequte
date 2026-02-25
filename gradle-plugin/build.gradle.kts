plugins {
    `java-gradle-plugin`
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.plugin.publish)
    jacoco
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
    compileOnly(gradleKotlinDsl())
    testImplementation(platform(libs.junit.bom))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
    finalizedBy(tasks.named("jacocoTestReport"))
}

tasks.named<JacocoReport>("jacocoTestReport") {
    reports {
        xml.required.set(true)
    }
}
