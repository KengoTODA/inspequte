package io.github.kengotoda.inspequte.gradle

import org.gradle.api.plugins.JavaPluginExtension
import org.gradle.testfixtures.ProjectBuilder
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class InspequtePluginTest {
    private fun mainInspequteTaskName(project: org.gradle.api.Project): String {
        val sourceSets = project.extensions.getByType(JavaPluginExtension::class.java).sourceSets
        return sourceSets.getByName("main").getTaskName("inspequte", null)
    }

    private fun taskArgs(project: org.gradle.api.Project): List<String> {
        val task = project.tasks.getByName(mainInspequteTaskName(project)) as InspequteTask
        return task.argumentProviders.flatMap { it.asArguments() }
    }

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

    @Test
    fun `registers inspequte extension`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply(InspequtePlugin::class.java)

        val extension = project.extensions.findByType(InspequteExtension::class.java)

        assertTrue(extension != null, "Expected 'inspequte' extension to be registered.")
    }

    @Test
    fun `forwards extension otel url to inspequte arguments`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val extension = project.extensions.getByType(InspequteExtension::class.java)
        extension.otel.set("http://localhost:8080")

        val args = taskArgs(project)

        assertTrue(args.contains("--otel"))
        assertTrue(args.windowed(size = 2, step = 1).any { it[0] == "--otel" && it[1] == "http://localhost:8080" })
    }

    @Test
    fun `task option overrides extension otel url`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val extension = project.extensions.getByType(InspequteExtension::class.java)
        extension.otel.set("http://localhost:8080")
        val task = project.tasks.getByName(mainInspequteTaskName(project)) as InspequteTask

        task.setInspequteOtel("http://localhost:4318/")
        val args = taskArgs(project)

        assertEquals("http://localhost:4318/", task.otel.get())
        assertTrue(args.windowed(size = 2, step = 1).any {
            it[0] == "--otel" && it[1] == "http://localhost:4318/"
        })
    }

    @Test
    fun `forwards default automation details id to inspequte arguments`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)

        val args = taskArgs(project)

        assertTrue(args.contains("--automation-details-id"))
        assertTrue(args.windowed(size = 2, step = 1).any {
            it[0] == "--automation-details-id" && it[1] == "inspequte/./main"
        })
    }

    @Test
    fun `forwards extension automation details id prefix to inspequte arguments`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val extension = project.extensions.getByType(InspequteExtension::class.java)
        extension.automationDetailsIdPrefix.set("inspequte/custom-path")

        val args = taskArgs(project)

        assertTrue(args.windowed(size = 2, step = 1).any {
            it[0] == "--automation-details-id" && it[1] == "inspequte/custom-path/main"
        })
    }

    @Test
    fun `task option overrides automation details id`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val task = project.tasks.getByName(mainInspequteTaskName(project)) as InspequteTask

        task.setInspequteAutomationDetailsId("inspequte/override/main")
        val args = taskArgs(project)

        assertEquals("inspequte/override/main", task.automationDetailsId.get())
        assertTrue(args.windowed(size = 2, step = 1).any {
            it[0] == "--automation-details-id" && it[1] == "inspequte/override/main"
        })
    }

    @Test
    fun `empty task automation details id disables forwarding`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val task = project.tasks.getByName(mainInspequteTaskName(project)) as InspequteTask

        task.setInspequteAutomationDetailsId("   ")
        val args = taskArgs(project)

        assertFalse(args.contains("--automation-details-id"))
    }

    @Test
    fun `does not forward allow duplicate classes by default`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)

        val args = taskArgs(project)

        assertFalse(args.contains("--allow-duplicate-classes"))
    }

    @Test
    fun `forwards extension allow duplicate classes to inspequte arguments`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val extension = project.extensions.getByType(InspequteExtension::class.java)
        extension.allowDuplicateClasses.set(true)

        val args = taskArgs(project)

        assertTrue(args.contains("--allow-duplicate-classes"))
    }

    @Test
    fun `task option enables allow duplicate classes`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val task = project.tasks.getByName(mainInspequteTaskName(project)) as InspequteTask

        task.setInspequteAllowDuplicateClasses(true)
        val args = taskArgs(project)

        assertTrue(task.allowDuplicateClasses.get())
        assertTrue(args.contains("--allow-duplicate-classes"))
    }

    @Test
    fun `fails for invalid otel url`() {
        val project = ProjectBuilder.builder().build()
        project.plugins.apply("java")
        project.plugins.apply(InspequtePlugin::class.java)
        val extension = project.extensions.getByType(InspequteExtension::class.java)
        extension.otel.set("localhost:4318/")
        val task = project.tasks.getByName(mainInspequteTaskName(project)) as InspequteTask

        val exception = assertThrows(IllegalArgumentException::class.java) {
            task.argumentProviders.flatMap { it.asArguments() }
        }

        assertTrue(exception.message.orEmpty().contains("Invalid OpenTelemetry collector URL"))
    }
}
