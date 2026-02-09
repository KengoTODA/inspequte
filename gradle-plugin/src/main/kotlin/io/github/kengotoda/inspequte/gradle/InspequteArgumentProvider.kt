package io.github.kengotoda.inspequte.gradle

import org.gradle.api.file.RegularFile
import org.gradle.api.provider.Provider
import org.gradle.process.CommandLineArgumentProvider

/**
 * Lazy command-line arguments for the inspequte Exec task.
 */
class InspequteArgumentProvider(
    private val writeInputsTask: Provider<WriteInspequteInputsTask>,
    private val reportFile: Provider<RegularFile>
) : CommandLineArgumentProvider {
    override fun asArguments(): Iterable<String> {
        val inputsPath = writeInputsTask.get().inputsFile.get().asFile.absolutePath
        val classpathPath = writeInputsTask.get().classpathFile.get().asFile.absolutePath
        val reportPath = reportFile.get().asFile.absolutePath

        return listOf(
            "--input", "@$inputsPath",
            "--classpath", "@$classpathPath",
            "--output", reportPath
        )
    }
}
