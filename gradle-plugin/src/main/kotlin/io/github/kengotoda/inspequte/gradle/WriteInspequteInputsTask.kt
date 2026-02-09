package io.github.kengotoda.inspequte.gradle

import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction

/**
 * Task that writes input and classpath files consumed by inspequte.
 */
abstract class WriteInspequteInputsTask : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val classDirectories: ConfigurableFileCollection

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val runtimeClasspath: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @get:OutputFile
    abstract val inputsFile: RegularFileProperty

    @get:OutputFile
    abstract val classpathFile: RegularFileProperty

    init {
        inputsFile.convention(outputDir.file("inputs.txt"))
        classpathFile.convention(outputDir.file("classpath.txt"))
    }

    @TaskAction
    fun writeInputs() {
        val inputs = classDirectories.files.map { it.absolutePath }.sorted()
        val classpath = runtimeClasspath.files.map { it.absolutePath }.sorted()

        val inputsOutput = inputsFile.get().asFile
        val classpathOutput = classpathFile.get().asFile

        inputsOutput.parentFile.mkdirs()
        classpathOutput.parentFile.mkdirs()

        inputsOutput.writeText(inputs.joinToString("\n"))
        classpathOutput.writeText(classpath.joinToString("\n"))
    }
}
