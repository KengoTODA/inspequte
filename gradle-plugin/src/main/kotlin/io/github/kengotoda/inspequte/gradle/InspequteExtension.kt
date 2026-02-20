package io.github.kengotoda.inspequte.gradle

import org.gradle.api.model.ObjectFactory
import org.gradle.api.provider.Property
import javax.inject.Inject

/**
 * Extension for configuring inspequte Gradle tasks.
 */
abstract class InspequteExtension @Inject constructor(objects: ObjectFactory) {
    /**
     * Optional OpenTelemetry collector URL passed to the CLI via `--otel`.
     */
    val otel: Property<String> = objects.property(String::class.java)

    /**
     * Optional prefix for `--automation-details-id`; each source set appends `/<sourceSetName>`.
     */
    val automationDetailsIdPrefix: Property<String> = objects.property(String::class.java)
}
