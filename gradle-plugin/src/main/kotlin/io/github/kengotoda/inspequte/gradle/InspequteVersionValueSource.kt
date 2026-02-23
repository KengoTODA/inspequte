package io.github.kengotoda.inspequte.gradle

import java.io.ByteArrayOutputStream
import javax.inject.Inject
import org.gradle.api.logging.Logging
import org.gradle.api.provider.ValueSource
import org.gradle.api.provider.ValueSourceParameters
import org.gradle.process.ExecOperations

/**
 * ValueSource that returns the version string reported by the inspequte executable.
 * Returns null when the command is unavailable or exits with a non-zero status.
 */
abstract class InspequteVersionValueSource : ValueSource<String, ValueSourceParameters.None> {
    @get:Inject
    abstract val execOperations: ExecOperations

    override fun obtain(): String? {
        return try {
            val stdout = ByteArrayOutputStream()
            val result = execOperations.exec { spec ->
                spec.commandLine("inspequte", "--version")
                spec.standardOutput = stdout
                spec.isIgnoreExitValue = true
            }
            if (result.exitValue == 0) stdout.toString().trim().ifEmpty { null } else null
        } catch (e: Exception) {
            Logging.getLogger(InspequteVersionValueSource::class.java)
                .debug("Failed to determine inspequte version", e)
            null
        }
    }
}
