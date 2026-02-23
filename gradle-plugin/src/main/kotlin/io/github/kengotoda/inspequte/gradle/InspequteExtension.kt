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

    /**
     * When `true`, duplicate class names across input artifacts emit a warning and the class from
     * the lexicographically first artifact path is used, instead of failing the build. Corresponds
     * to the CLI flag `--allow-duplicate-classes`. Defaults to `false`.
     */
    val allowDuplicateClasses: Property<Boolean> = objects.property(Boolean::class.java)
}
