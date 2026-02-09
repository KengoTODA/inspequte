package io.github.kengotoda.inspequte.gradle

import org.gradle.api.plugins.JavaPluginExtension
import org.gradle.testfixtures.ProjectBuilder
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class InspequtePluginTest {
    @Test
    fun `registers inspequte tasks for java source sets`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")

        project.plugins.apply(InspequtePlugin::class.java)

        val sourceSets = project.extensions.getByType(JavaPluginExtension::class.java).sourceSets
        val expectedTasks = sourceSets.flatMap { sourceSet ->
            listOf(
                sourceSet.getTaskName("writeInspequteInputs", null),
                sourceSet.getTaskName("inspequte", null)
            )
        }

        expectedTasks.forEach { taskName ->
            assertTrue(project.tasks.names.contains(taskName), "Expected task '$taskName' to be registered.")
        }
    }

    @Test
    fun `does not register tasks when java-base is missing`() {
        val project = ProjectBuilder.builder().build()

        project.plugins.apply(InspequtePlugin::class.java)

        assertNull(project.tasks.findByName("writeInspequteInputs"))
        assertNull(project.tasks.findByName("inspequte"))
        assertFalse(project.tasks.names.any { it.startsWith("writeInspequteInputs") })
        assertFalse(project.tasks.names.any { it.startsWith("inspequte") })
    }
}
