import org.gradle.api.publish.PublishingExtension
import org.gradle.api.publish.maven.MavenPublication
import org.gradle.api.publish.maven.tasks.GenerateMavenPom
import org.gradle.api.publish.tasks.GenerateModuleMetadata
import org.gradle.jvm.tasks.Jar

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

// Publication uses these exact files, including the plugin marker POM. The
// repository layout is retained when transferring them to the publishing job.
fun releasePublicationFiles(): Set<File> {
    val publicationFiles = extensions.getByType<PublishingExtension>()
        .publications.withType<MavenPublication>().flatMap { publication ->
            publication.artifacts.map { it.file }
        } + tasks.withType<GenerateMavenPom>().map { it.destination }
    check(publicationFiles.isNotEmpty()) { "No release publication files were generated" }
    publicationFiles.forEach { file ->
        check(file.isFile) { "Missing release publication file: $file" }
    }
    // Marker publications have no component, so their module metadata task may
    // be skipped. Include every metadata file that is actually generated.
    return (publicationFiles + tasks.withType<GenerateModuleMetadata>()
        .flatMap { it.outputs.files.files }.filter { it.isFile }).toSet()
}

tasks.register("prepareReleaseArtifacts") {
    dependsOn(tasks.withType<Jar>(), tasks.withType<GenerateMavenPom>(),
        tasks.withType<GenerateModuleMetadata>())
    val destination = layout.buildDirectory.dir("release-artifacts")
    outputs.dir(destination)
    doLast {
        val buildDirectory = layout.buildDirectory.get().asFile
        delete(destination)
        releasePublicationFiles().forEach { file ->
            val relativePath = file.relativeTo(buildDirectory)
            copy {
                from(file)
                into(destination.get().dir(relativePath.parent ?: ""))
            }
        }
    }
}

val verifyRelease = providers.gradleProperty("verifyRelease").isPresent
if (verifyRelease) {
    // A publishing job restores the attested files. Do not regenerate them.
    tasks.withType<Jar>().configureEach { enabled = false }
    tasks.withType<GenerateMavenPom>().configureEach { enabled = false }
    tasks.withType<GenerateModuleMetadata>().configureEach { enabled = false }
}

tasks.named("publishPlugins") {
    doFirst {
        if (verifyRelease) {
            val sourceSha = providers.environmentVariable("GITHUB_SHA").get()
            val command = listOf(
                "bash", layout.projectDirectory.file("../scripts/verify-release-artifacts.sh").asFile.absolutePath,
                "build-gradle-release.yml", sourceSha, sourceSha
            ) + releasePublicationFiles().map { it.absolutePath }
            val exitCode = ProcessBuilder(command).inheritIO().start().waitFor()
            check(exitCode == 0) { "Release attestation verification failed; publication stopped" }
        }
    }
}
