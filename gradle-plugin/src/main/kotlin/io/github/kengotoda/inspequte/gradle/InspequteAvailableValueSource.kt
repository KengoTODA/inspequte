package io.github.kengotoda.inspequte.gradle

import org.gradle.api.provider.ValueSource
import org.gradle.api.provider.ValueSourceParameters
import org.gradle.process.ExecOperations
import java.io.ByteArrayOutputStream
import java.util.Locale
import javax.inject.Inject

/**
 * ValueSource that checks whether the inspequte executable is available.
 */
abstract class InspequteAvailableValueSource : ValueSource<Boolean, ValueSourceParameters.None> {
    @get:Inject
    abstract val execOperations: ExecOperations

    override fun obtain(): Boolean {
        return try {
            val stdout = ByteArrayOutputStream()
            val stderr = ByteArrayOutputStream()
            val command = if (System.getProperty("os.name").lowercase(Locale.ENGLISH).contains("win")) {
                listOf("where", "inspequte")
            } else {
                listOf("which", "inspequte")
            }

            val result = execOperations.exec {
                commandLine(command)
                standardOutput = stdout
                errorOutput = stderr
                isIgnoreExitValue = true
            }

            result.exitValue == 0
        } catch (_: Exception) {
            false
        }
    }
}
