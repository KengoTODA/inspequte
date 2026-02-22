package io.github.kengotoda.inspequte.gradle

import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.Optional
import org.gradle.api.tasks.options.Option
import org.gradle.api.tasks.Exec

/**
 * Task that runs inspequte with optional OpenTelemetry export configuration.
 */
abstract class InspequteTask : Exec() {
    /**
     * Optional OpenTelemetry collector URL passed as `--otel`.
     */
    @get:Input
    @get:Optional
    abstract val otel: Property<String>

    /**
     * Optional SARIF run automation details ID passed as `--automation-details-id`.
     */
    @get:Input
    @get:Optional
    abstract val automationDetailsId: Property<String>

    /**
     * When `true`, passes `--allow-duplicate-classes` to the CLI so that duplicate class names
     * across input artifacts produce a warning instead of a build failure.
     */
    @get:Input
    abstract val allowDuplicateClasses: Property<Boolean>

    @Option(
        option = "inspequte-otel",
        description = "OpenTelemetry collector URL forwarded to inspequte --otel."
    )
    fun setInspequteOtel(value: String) {
        otel.set(value)
    }

    @Option(
        option = "inspequte-automation-details-id",
        description = "SARIF run automation details ID forwarded to inspequte --automation-details-id."
    )
    fun setInspequteAutomationDetailsId(value: String) {
        automationDetailsId.set(value)
    }

    @Option(
        option = "inspequte-allow-duplicate-classes",
        description = "Warn instead of failing when the same class name appears in multiple inputs."
    )
    fun setInspequteAllowDuplicateClasses(value: Boolean) {
        allowDuplicateClasses.set(value)
    }
}
