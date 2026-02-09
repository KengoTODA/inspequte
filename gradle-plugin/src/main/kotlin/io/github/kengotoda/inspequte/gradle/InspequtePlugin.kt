package io.github.kengotoda.inspequte.gradle

import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.plugins.JavaBasePlugin
import org.gradle.api.plugins.JavaPluginExtension
import org.gradle.api.provider.Provider
import org.gradle.api.tasks.Exec
import org.gradle.api.tasks.SourceSet
import org.gradle.kotlin.dsl.getByType
import org.gradle.kotlin.dsl.register

/**
 * Gradle plugin that registers inspequte analysis tasks for each Java source set.
 */
class InspequtePlugin : Plugin<Project> {
    override fun apply(project: Project) {
        val inspequteAvailable = project.providers.of(InspequteAvailableValueSource::class.java) {}

        project.pluginManager.withPlugin("java-base") {
            val javaExtension = project.extensions.getByType<JavaPluginExtension>()
            javaExtension.sourceSets.configureEach {
                configureInspequteForSourceSet(project, this, inspequteAvailable)
            }
        }
    }

    private fun configureInspequteForSourceSet(
        project: Project,
        sourceSet: SourceSet,
        inspequteAvailable: Provider<Boolean>
    ) {
        val outputDir = project.layout.buildDirectory.dir("inspequte/${sourceSet.name}")
        val writeInputsTaskName = sourceSet.getTaskName("writeInspequteInputs", null)
        val inspequteTaskName = sourceSet.getTaskName("inspequte", null)

        val writeInputsTask = project.tasks.register<WriteInspequteInputsTask>(writeInputsTaskName) {
            classDirectories.from(sourceSet.output.classesDirs)
            runtimeClasspath.from(sourceSet.runtimeClasspath)
            this.outputDir.set(outputDir)

            dependsOn(project.tasks.named(sourceSet.classesTaskName))
            group = JavaBasePlugin.VERIFICATION_GROUP
            description = "Writes inspequte input files for the '${sourceSet.name}' source set."
        }

        val inspequteTask = project.tasks.register<Exec>(inspequteTaskName) {
            dependsOn(writeInputsTask)
            group = JavaBasePlugin.VERIFICATION_GROUP
            description = "Runs inspequte for the '${sourceSet.name}' source set."

            val reportFile = outputDir.map { it.file("report.sarif") }
            inputs.files(writeInputsTask.flatMap { it.inputsFile }, writeInputsTask.flatMap { it.classpathFile })
            outputs.file(reportFile)

            onlyIf {
                val available = inspequteAvailable.get()
                if (!available) {
                    project.logger.warn(
                        "Skipping '${name}': the 'inspequte' command is not available in PATH. " +
                            "Install it with: cargo install inspequte --locked"
                    )
                }
                available
            }

            executable("inspequte")
            argumentProviders.add(InspequteArgumentProvider(writeInputsTask, reportFile))
        }

        project.tasks.named(JavaBasePlugin.CHECK_TASK_NAME) {
            dependsOn(inspequteTask)
        }
    }
}
