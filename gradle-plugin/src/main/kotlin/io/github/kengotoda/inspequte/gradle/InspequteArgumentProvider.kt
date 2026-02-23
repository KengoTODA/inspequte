package io.github.kengotoda.inspequte.gradle

import java.net.URI
import org.gradle.api.file.RegularFile
import org.gradle.api.provider.Provider
import org.gradle.process.CommandLineArgumentProvider

/**
 * Lazy command-line arguments for the inspequte Exec task.
 */
class InspequteArgumentProvider(
    private val writeInputsTask: Provider<WriteInspequteInputsTask>,
    private val reportFile: Provider<RegularFile>,
    private val otelUrl: Provider<String>,
    private val automationDetailsId: Provider<String>,
    private val allowDuplicateClasses: Provider<Boolean>
) : CommandLineArgumentProvider {
    override fun asArguments(): Iterable<String> {
        val inputsPath = writeInputsTask.get().inputsFile.get().asFile.absolutePath
        val classpathPath = writeInputsTask.get().classpathFile.get().asFile.absolutePath
        val reportPath = reportFile.get().asFile.absolutePath
        val args = mutableListOf(
            "--input", "@$inputsPath",
            "--classpath", "@$classpathPath",
            "--output", reportPath
        )
        if (otelUrl.isPresent) {
            val url = otelUrl.get().trim()
            if (url.isNotEmpty()) {
                validateOtelUrl(url)
                args.add("--otel")
                args.add(url)
            }
        }
        if (automationDetailsId.isPresent) {
            val id = automationDetailsId.get().trim()
            if (id.isNotEmpty()) {
                args.add("--automation-details-id")
                args.add(id)
            }
        }
        if (allowDuplicateClasses.get()) {
            args.add("--allow-duplicate-classes")
        }

        return args
    }

    private fun validateOtelUrl(url: String) {
        val uri = try {
            URI(url)
        } catch (_: Exception) {
            throw invalidOtelUrl(url)
        }
        if (!uri.isAbsolute || (uri.scheme != "http" && uri.scheme != "https") || uri.host.isNullOrBlank()) {
            throw invalidOtelUrl(url)
        }
    }

    private fun invalidOtelUrl(url: String): IllegalArgumentException {
        return IllegalArgumentException(
            "Invalid OpenTelemetry collector URL '$url'. " +
                "Expected absolute http(s) URL, e.g. http://localhost:4318/."
        )
    }
}
