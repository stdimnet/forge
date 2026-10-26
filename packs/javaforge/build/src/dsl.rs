//! Completion in Gradle build scripts (Kotlin DSL `*.gradle.kts` and Groovy DSL `*.gradle`)
//! and in `pom.xml`, like IntelliJ IDEA's.
//!
//! - Gradle: a hand-written model of the common blocks and functions (`plugins`,
//!   `dependencies` and its configurations, `repositories`, `java { toolchain }`,
//!   `tasks.register`, `application`, `jacoco`, `test`, settings scripts …), chosen by the
//!   block the cursor is in; members after `tasks.`, `JavaLanguageVersion.`, …; plugin ids.
//! - Dependency coordinates and versions in strings of a dependency configuration and in
//!   `<groupId>`, `<artifactId>` and `<version>` of `pom.xml`, from Maven Central
//!   ([`request`] says what to look up, [`complete`] builds the items from the answer, with
//!   a few well-known artifacts when Maven Central gives nothing).
//!
//! Offsets in and out are UTF-16 (the editor's).

use forge_jsonschema::complete::Item;
use forge_jsonschema::text::{byte_offset, Utf16};
use forge_jsonschema::CompletionOut;
use serde::{Deserialize, Serialize};

use crate::central::{Artifact, Versions};
use crate::deps::blank_comments;
use crate::version;

/// The kind of file being completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Kotlin { settings: bool },
    Groovy { settings: bool },
    Pom,
}

impl Script {
    pub fn of_path(path: &str) -> Option<Script> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let settings = name.starts_with("settings.gradle");
        if name.ends_with(".gradle.kts") {
            Some(Script::Kotlin { settings })
        } else if name.ends_with(".gradle") {
            Some(Script::Groovy { settings })
        } else if name == "pom.xml" {
            Some(Script::Pom)
        } else {
            None
        }
    }
}

/// What Maven Central must answer for the completion at the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Request {
    /// Artifacts matching what was typed (`guav`, `org.junit.jupiter:`).
    Coordinates { prefix: String },
    /// Versions of `group:artifact`.
    Versions {
        group: String,
        artifact: String,
        prefix: String,
    },
}

/// Maven Central's answer for a [`Request`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CentralData {
    pub artifacts: Vec<Artifact>,
    pub versions: Option<Versions>,
}

/// Well-known artifacts offered without Maven Central (offline, or before it answers).
const POPULAR: [(&str, &str, &str); 24] = [
    ("com.google.guava", "guava", "33.3.1-jre"),
    ("org.junit.jupiter", "junit-jupiter", "5.11.3"),
    ("org.junit", "junit-bom", "5.11.3"),
    ("org.junit.platform", "junit-platform-launcher", "1.11.3"),
    ("junit", "junit", "4.13.2"),
    ("org.testng", "testng", "7.10.2"),
    ("org.assertj", "assertj-core", "3.26.3"),
    ("org.mockito", "mockito-core", "5.14.2"),
    ("org.mockito", "mockito-junit-jupiter", "5.14.2"),
    ("org.slf4j", "slf4j-api", "2.0.16"),
    ("ch.qos.logback", "logback-classic", "1.5.12"),
    ("org.apache.commons", "commons-lang3", "3.17.0"),
    ("commons-io", "commons-io", "2.17.0"),
    ("com.fasterxml.jackson.core", "jackson-databind", "2.18.1"),
    ("com.google.code.gson", "gson", "2.11.0"),
    ("org.projectlombok", "lombok", "1.18.34"),
    (
        "org.springframework.boot",
        "spring-boot-starter-web",
        "3.3.5",
    ),
    (
        "org.springframework.boot",
        "spring-boot-starter-test",
        "3.3.5",
    ),
    ("org.postgresql", "postgresql", "42.7.4"),
    ("com.h2database", "h2", "2.3.232"),
    ("org.xerial", "sqlite-jdbc", "3.47.0.0"),
    ("com.squareup.okhttp3", "okhttp", "4.12.0"),
    ("org.jetbrains.kotlin", "kotlin-stdlib", "2.0.21"),
    ("info.picocli", "picocli", "4.7.6"),
];

/// Plugin ids offered in `plugins { id("…") }`.
const PLUGIN_IDS: [(&str, &str); 14] = [
    ("java", "Java compilation, testing and bundling"),
    ("java-library", "A Java library with an API"),
    (
        "application",
        "A runnable JVM application (run, distributions)",
    ),
    ("jacoco", "JaCoCo code coverage"),
    ("maven-publish", "Publishing to Maven repositories"),
    (
        "java-test-fixtures",
        "Test fixtures shared between projects",
    ),
    ("checkstyle", "Checkstyle code quality checks"),
    ("idea", "IntelliJ IDEA project files"),
    ("org.springframework.boot", "Spring Boot"),
    (
        "io.spring.dependency-management",
        "Spring's dependency management",
    ),
    ("com.gradleup.shadow", "Fat JARs (Shadow)"),
    ("org.jetbrains.kotlin.jvm", "Kotlin for the JVM"),
    ("com.diffplug.spotless", "Spotless code formatting"),
    ("org.graalvm.buildtools.native", "GraalVM native images"),
];

/// Dependency configurations of the `java` plugins.
const CONFIGURATIONS: [(&str, &str); 10] = [
    (
        "implementation",
        "Implementation only dependencies for source set 'main'",
    ),
    (
        "api",
        "API dependencies for source set 'main' (java-library)",
    ),
    (
        "compileOnly",
        "Compile only dependencies for source set 'main'",
    ),
    (
        "runtimeOnly",
        "Runtime only dependencies for source set 'main'",
    ),
    (
        "annotationProcessor",
        "Annotation processors and their dependencies for source set 'main'",
    ),
    (
        "testImplementation",
        "Implementation only dependencies for source set 'test'",
    ),
    (
        "testCompileOnly",
        "Compile only dependencies for source set 'test'",
    ),
    (
        "testRuntimeOnly",
        "Runtime only dependencies for source set 'test'",
    ),
    (
        "testAnnotationProcessor",
        "Annotation processors for source set 'test'",
    ),
    (
        "developmentOnly",
        "Dependencies for development only (Spring Boot)",
    ),
];

/// An entry of the DSL model: label, detail, Kotlin insert text, Groovy insert text.
type Entry = (&'static str, &'static str, &'static str, &'static str);

const TOP: [Entry; 17] = [
    (
        "plugins",
        "Plugins of the project",
        "plugins {\n\t$0\n}",
        "plugins {\n\t$0\n}",
    ),
    (
        "dependencies",
        "Dependencies of the project",
        "dependencies {\n\t$0\n}",
        "dependencies {\n\t$0\n}",
    ),
    (
        "repositories",
        "Repositories dependencies are resolved from",
        "repositories {\n\t$0\n}",
        "repositories {\n\t$0\n}",
    ),
    (
        "java",
        "Java plugin extension (toolchain, compatibility)",
        "java {\n\t$0\n}",
        "java {\n\t$0\n}",
    ),
    (
        "application",
        "Application plugin extension (mainClass)",
        "application {\n\t$0\n}",
        "application {\n\t$0\n}",
    ),
    (
        "jacoco",
        "JaCoCo plugin extension",
        "jacoco {\n\t$0\n}",
        "jacoco {\n\t$0\n}",
    ),
    ("tasks", "The tasks of the project", "tasks", "tasks"),
    (
        "group",
        "The group of the project",
        "group = \"$1\"",
        "group = '$1'",
    ),
    (
        "version",
        "The version of the project",
        "version = \"$1\"",
        "version = '$1'",
    ),
    (
        "description",
        "The description of the project",
        "description = \"$1\"",
        "description = '$1'",
    ),
    (
        "sourceSets",
        "Source sets (main, test)",
        "sourceSets {\n\t$0\n}",
        "sourceSets {\n\t$0\n}",
    ),
    (
        "configurations",
        "Dependency configurations",
        "configurations {\n\t$0\n}",
        "configurations {\n\t$0\n}",
    ),
    (
        "buildscript",
        "Classpath of the build script",
        "buildscript {\n\t$0\n}",
        "buildscript {\n\t$0\n}",
    ),
    (
        "allprojects",
        "Configures this project and its subprojects",
        "allprojects {\n\t$0\n}",
        "allprojects {\n\t$0\n}",
    ),
    (
        "subprojects",
        "Configures the subprojects",
        "subprojects {\n\t$0\n}",
        "subprojects {\n\t$0\n}",
    ),
    (
        "testing",
        "Test suites (jvm-test-suite)",
        "testing {\n\t$0\n}",
        "testing {\n\t$0\n}",
    ),
    (
        "test",
        "The test task",
        "tasks.test {\n\t$0\n}",
        "test {\n\t$0\n}",
    ),
];

const SETTINGS: [Entry; 6] = [
    (
        "rootProject.name",
        "Name of the root project",
        "rootProject.name = \"$1\"",
        "rootProject.name = '$1'",
    ),
    (
        "include",
        "Adds subprojects to the build",
        "include(\"$1\")",
        "include '$1'",
    ),
    (
        "includeBuild",
        "Includes another build (composite build)",
        "includeBuild(\"$1\")",
        "includeBuild '$1'",
    ),
    (
        "pluginManagement",
        "Repositories and versions of plugins",
        "pluginManagement {\n\t$0\n}",
        "pluginManagement {\n\t$0\n}",
    ),
    (
        "dependencyResolutionManagement",
        "Repositories and version catalogs of all projects",
        "dependencyResolutionManagement {\n\t$0\n}",
        "dependencyResolutionManagement {\n\t$0\n}",
    ),
    (
        "plugins",
        "Settings plugins",
        "plugins {\n\t$0\n}",
        "plugins {\n\t$0\n}",
    ),
];

const PLUGINS: [Entry; 9] = [
    ("java", "Java plugin", "java", "id 'java'"),
    (
        "java-library",
        "Java library plugin",
        "`java-library`",
        "id 'java-library'",
    ),
    (
        "application",
        "Application plugin",
        "application",
        "id 'application'",
    ),
    ("jacoco", "JaCoCo plugin", "jacoco", "id 'jacoco'"),
    (
        "maven-publish",
        "Maven publish plugin",
        "`maven-publish`",
        "id 'maven-publish'",
    ),
    ("id", "A plugin by its id", "id(\"$1\")", "id '$1'"),
    (
        "id … version",
        "A plugin by its id and version",
        "id(\"$1\") version \"$2\"",
        "id '$1' version '$2'",
    ),
    (
        "kotlin",
        "A Kotlin plugin",
        "kotlin(\"$1\") version \"$2\"",
        "id 'org.jetbrains.kotlin.$1' version '$2'",
    ),
    (
        "alias",
        "A plugin from the version catalog",
        "alias(libs.plugins.$1)",
        "alias(libs.plugins.$1)",
    ),
];

const REPOSITORIES: [Entry; 5] = [
    (
        "mavenCentral",
        "Maven Central",
        "mavenCentral()",
        "mavenCentral()",
    ),
    (
        "google",
        "Google's Maven repository",
        "google()",
        "google()",
    ),
    (
        "mavenLocal",
        "The local Maven repository (~/.m2)",
        "mavenLocal()",
        "mavenLocal()",
    ),
    (
        "gradlePluginPortal",
        "The Gradle Plugin Portal",
        "gradlePluginPortal()",
        "gradlePluginPortal()",
    ),
    (
        "maven",
        "A Maven repository by URL",
        "maven {\n\turl = uri(\"$1\")\n}",
        "maven {\n\turl = '$1'\n}",
    ),
];

const JAVA: [Entry; 6] = [
    (
        "toolchain",
        "The Java toolchain that compiles, tests and runs",
        "toolchain {\n\tlanguageVersion = JavaLanguageVersion.of($1)\n}",
        "toolchain {\n\tlanguageVersion = JavaLanguageVersion.of($1)\n}",
    ),
    (
        "sourceCompatibility",
        "Java release of the sources",
        "sourceCompatibility = JavaVersion.VERSION_$1",
        "sourceCompatibility = JavaVersion.VERSION_$1",
    ),
    (
        "targetCompatibility",
        "Java release of the classes",
        "targetCompatibility = JavaVersion.VERSION_$1",
        "targetCompatibility = JavaVersion.VERSION_$1",
    ),
    (
        "withSourcesJar",
        "Publishes a -sources.jar",
        "withSourcesJar()",
        "withSourcesJar()",
    ),
    (
        "withJavadocJar",
        "Publishes a -javadoc.jar",
        "withJavadocJar()",
        "withJavadocJar()",
    ),
    (
        "modularity.inferModulePath",
        "Uses the module path for modules",
        "modularity.inferModulePath = true",
        "modularity.inferModulePath = true",
    ),
];

const TOOLCHAIN: [Entry; 3] = [
    (
        "languageVersion",
        "Java release of the toolchain",
        "languageVersion = JavaLanguageVersion.of($1)",
        "languageVersion = JavaLanguageVersion.of($1)",
    ),
    (
        "vendor",
        "JDK vendor",
        "vendor = JvmVendorSpec.$1",
        "vendor = JvmVendorSpec.$1",
    ),
    (
        "implementation",
        "JVM implementation",
        "implementation = JvmImplementation.$1",
        "implementation = JvmImplementation.$1",
    ),
];

const APPLICATION: [Entry; 4] = [
    (
        "mainClass",
        "The main class of the application",
        "mainClass = \"$1\"",
        "mainClass = '$1'",
    ),
    (
        "mainModule",
        "The main module (JPMS)",
        "mainModule = \"$1\"",
        "mainModule = '$1'",
    ),
    (
        "applicationDefaultJvmArgs",
        "JVM arguments of run and the start scripts",
        "applicationDefaultJvmArgs = listOf(\"$1\")",
        "applicationDefaultJvmArgs = ['$1']",
    ),
    (
        "applicationName",
        "Name of the start scripts",
        "applicationName = \"$1\"",
        "applicationName = '$1'",
    ),
];

const JACOCO: [Entry; 2] = [
    (
        "toolVersion",
        "JaCoCo version",
        "toolVersion = \"$1\"",
        "toolVersion = '$1'",
    ),
    (
        "reportsDirectory",
        "Where the reports go",
        "reportsDirectory = layout.buildDirectory.dir(\"$1\")",
        "reportsDirectory = layout.buildDirectory.dir('$1')",
    ),
];

const TASKS: [Entry; 8] = [
    (
        "register",
        "Registers a new task",
        "register<$1>(\"$2\") {\n\t$0\n}",
        "register('$2', $1) {\n\t$0\n}",
    ),
    (
        "named",
        "Configures a task by its name",
        "named(\"$1\") {\n\t$0\n}",
        "named('$1') {\n\t$0\n}",
    ),
    (
        "withType",
        "Configures all tasks of a type",
        "withType<$1> {\n\t$0\n}",
        "withType($1) {\n\t$0\n}",
    ),
    (
        "test",
        "The test task",
        "test {\n\t$0\n}",
        "test {\n\t$0\n}",
    ),
    ("jar", "The jar task", "jar {\n\t$0\n}", "jar {\n\t$0\n}"),
    (
        "javadoc",
        "The javadoc task",
        "javadoc {\n\t$0\n}",
        "javadoc {\n\t$0\n}",
    ),
    (
        "jacocoTestReport",
        "The JaCoCo report task",
        "jacocoTestReport {\n\t$0\n}",
        "jacocoTestReport {\n\t$0\n}",
    ),
    (
        "compileJava",
        "The compileJava task",
        "compileJava {\n\t$0\n}",
        "compileJava {\n\t$0\n}",
    ),
];

const TEST: [Entry; 10] = [
    (
        "useJUnitPlatform",
        "Runs JUnit 5 (Jupiter) and other JUnit Platform engines",
        "useJUnitPlatform()",
        "useJUnitPlatform()",
    ),
    ("useTestNG", "Runs TestNG", "useTestNG()", "useTestNG()"),
    ("useJUnit", "Runs JUnit 4", "useJUnit()", "useJUnit()"),
    (
        "maxParallelForks",
        "Test JVMs running in parallel",
        "maxParallelForks = $1",
        "maxParallelForks = $1",
    ),
    (
        "jvmArgs",
        "JVM arguments of the test JVM",
        "jvmArgs(\"$1\")",
        "jvmArgs '$1'",
    ),
    (
        "systemProperty",
        "A system property of the test JVM",
        "systemProperty(\"$1\", \"$2\")",
        "systemProperty '$1', '$2'",
    ),
    (
        "testLogging",
        "What the console shows",
        "testLogging {\n\tevents(\"passed\", \"skipped\", \"failed\")\n}",
        "testLogging {\n\tevents 'passed', 'skipped', 'failed'\n}",
    ),
    (
        "finalizedBy",
        "Runs a task afterwards",
        "finalizedBy(tasks.$1)",
        "finalizedBy $1",
    ),
    (
        "filter",
        "Selects tests",
        "filter {\n\tincludeTestsMatching(\"$1\")\n}",
        "filter {\n\tincludeTestsMatching '$1'\n}",
    ),
    (
        "ignoreFailures",
        "Does not fail the build on test failures",
        "ignoreFailures = true",
        "ignoreFailures = true",
    ),
];

const JACOCO_REPORT: [Entry; 3] = [
    (
        "reports",
        "Report formats",
        "reports {\n\txml.required = true\n\thtml.required = true\n}",
        "reports {\n\txml.required = true\n\thtml.required = true\n}",
    ),
    (
        "dependsOn",
        "Runs after",
        "dependsOn(tasks.$1)",
        "dependsOn $1",
    ),
    (
        "executionData",
        "Execution data files",
        "executionData($1)",
        "executionData $1",
    ),
];

const REPORTS: [Entry; 3] = [
    (
        "xml.required",
        "Writes the XML report",
        "xml.required = true",
        "xml.required = true",
    ),
    (
        "html.required",
        "Writes the HTML report",
        "html.required = true",
        "html.required = true",
    ),
    (
        "csv.required",
        "Writes the CSV report",
        "csv.required = false",
        "csv.required = false",
    ),
];

const TASK: [Entry; 9] = [
    (
        "dependsOn",
        "Tasks that run first",
        "dependsOn($1)",
        "dependsOn $1",
    ),
    (
        "doLast",
        "An action at the end",
        "doLast {\n\t$0\n}",
        "doLast {\n\t$0\n}",
    ),
    (
        "doFirst",
        "An action at the start",
        "doFirst {\n\t$0\n}",
        "doFirst {\n\t$0\n}",
    ),
    (
        "group",
        "The task's group in the Gradle tool window",
        "group = \"$1\"",
        "group = '$1'",
    ),
    (
        "description",
        "The task's description",
        "description = \"$1\"",
        "description = '$1'",
    ),
    ("from", "Files to copy", "from(\"$1\")", "from '$1'"),
    (
        "into",
        "Destination of the copy",
        "into(\"$1\")",
        "into '$1'",
    ),
    (
        "enabled",
        "Whether the task runs",
        "enabled = false",
        "enabled = false",
    ),
    (
        "finalizedBy",
        "Runs a task afterwards",
        "finalizedBy($1)",
        "finalizedBy $1",
    ),
];

const JAR: [Entry; 3] = [
    (
        "manifest",
        "The JAR manifest",
        "manifest {\n\tattributes(\"Main-Class\" to \"$1\")\n}",
        "manifest {\n\tattributes 'Main-Class': '$1'\n}",
    ),
    (
        "archiveBaseName",
        "Base name of the archive",
        "archiveBaseName = \"$1\"",
        "archiveBaseName = '$1'",
    ),
    (
        "archiveVersion",
        "Version in the archive's name",
        "archiveVersion = \"$1\"",
        "archiveVersion = '$1'",
    ),
];

const MAVEN_REPO: [Entry; 3] = [
    ("url", "Repository URL", "url = uri(\"$1\")", "url = '$1'"),
    ("name", "Repository name", "name = \"$1\"", "name = '$1'"),
    (
        "credentials",
        "User name and password",
        "credentials {\n\tusername = $1\n\tpassword = $2\n}",
        "credentials {\n\tusername = $1\n\tpassword = $2\n}",
    ),
];

const BUILDSCRIPT: [Entry; 2] = [
    (
        "repositories",
        "Repositories of the build script's classpath",
        "repositories {\n\t$0\n}",
        "repositories {\n\t$0\n}",
    ),
    (
        "dependencies",
        "The build script's classpath",
        "dependencies {\n\t$0\n}",
        "dependencies {\n\t$0\n}",
    ),
];

const PROJECTS: [Entry; 6] = [
    (
        "repositories",
        "Repositories of the projects",
        "repositories {\n\t$0\n}",
        "repositories {\n\t$0\n}",
    ),
    (
        "dependencies",
        "Dependencies of the projects",
        "dependencies {\n\t$0\n}",
        "dependencies {\n\t$0\n}",
    ),
    (
        "apply",
        "Applies a plugin",
        "apply(plugin = \"$1\")",
        "apply plugin: '$1'",
    ),
    (
        "group",
        "The group of the projects",
        "group = \"$1\"",
        "group = '$1'",
    ),
    (
        "version",
        "The version of the projects",
        "version = \"$1\"",
        "version = '$1'",
    ),
    ("tasks", "Tasks of the projects", "tasks", "tasks"),
];

const SOURCE_SETS: [Entry; 2] = [
    (
        "main",
        "The production source set",
        "main {\n\t$0\n}",
        "main {\n\t$0\n}",
    ),
    (
        "test",
        "The test source set",
        "test {\n\t$0\n}",
        "test {\n\t$0\n}",
    ),
];

const SOURCE_SET: [Entry; 2] = [
    (
        "java",
        "Java sources",
        "java {\n\tsrcDir(\"$1\")\n}",
        "java {\n\tsrcDir '$1'\n}",
    ),
    (
        "resources",
        "Resources",
        "resources {\n\tsrcDir(\"$1\")\n}",
        "resources {\n\tsrcDir '$1'\n}",
    ),
];

const PLUGIN_MANAGEMENT: [Entry; 2] = [
    (
        "repositories",
        "Where plugins come from",
        "repositories {\n\t$0\n}",
        "repositories {\n\t$0\n}",
    ),
    (
        "plugins",
        "Plugin versions",
        "plugins {\n\t$0\n}",
        "plugins {\n\t$0\n}",
    ),
];

const RESOLUTION_MANAGEMENT: [Entry; 3] = [
    (
        "repositories",
        "Repositories of all projects",
        "repositories {\n\t$0\n}",
        "repositories {\n\t$0\n}",
    ),
    (
        "versionCatalogs",
        "Version catalogs",
        "versionCatalogs {\n\t$0\n}",
        "versionCatalogs {\n\t$0\n}",
    ),
    (
        "repositoriesMode",
        "Whether projects may declare repositories",
        "repositoriesMode = RepositoriesMode.FAIL_ON_PROJECT_REPOS",
        "repositoriesMode = RepositoriesMode.FAIL_ON_PROJECT_REPOS",
    ),
];

const DEPENDENCY_HELPERS: [Entry; 4] = [
    (
        "platform",
        "Versions from a BOM",
        "implementation(platform(\"$1\"))",
        "implementation platform('$1')",
    ),
    (
        "project",
        "Another project of the build",
        "implementation(project(\":$1\"))",
        "implementation project(':$1')",
    ),
    (
        "files",
        "Local files",
        "implementation(files(\"$1\"))",
        "implementation files('$1')",
    ),
    (
        "testFixtures",
        "Test fixtures of a project",
        "testImplementation(testFixtures(project(\":$1\")))",
        "testImplementation testFixtures(project(':$1'))",
    ),
];

/// Members after a receiver and a dot.
fn members(receiver: &str) -> &'static [(&'static str, &'static str)] {
    match receiver {
        "JavaLanguageVersion" => &[("of", "JavaLanguageVersion.of(21)")],
        "JvmVendorSpec" => &[
            ("ADOPTIUM", "Eclipse Temurin"),
            ("AMAZON", "Amazon Corretto"),
            ("AZUL", "Azul Zulu"),
            ("BELLSOFT", "BellSoft Liberica"),
            ("GRAAL_VM", "GraalVM"),
            ("IBM", "IBM Semeru"),
            ("MICROSOFT", "Microsoft Build of OpenJDK"),
            ("ORACLE", "Oracle OpenJDK"),
        ],
        "JavaVersion" => &[
            ("VERSION_17", "Java 17"),
            ("VERSION_21", "Java 21"),
            ("VERSION_25", "Java 25"),
            ("current", "The running JVM's"),
        ],
        "tasks" => &[
            ("register", "Registers a new task"),
            ("named", "A task by its name"),
            ("withType", "Tasks of a type"),
            ("test", "The test task"),
            ("jar", "The jar task"),
            ("javadoc", "The javadoc task"),
            ("jacocoTestReport", "The JaCoCo report task"),
            ("compileJava", "The compileJava task"),
            ("build", "The build task"),
            ("check", "The check task"),
        ],
        "layout" => &[
            ("buildDirectory", "build/"),
            ("projectDirectory", "The project's folder"),
        ],
        "rootProject" => &[("name", "Name of the root project")],
        "xml" | "html" | "csv" => &[
            ("required", "Whether the report is written"),
            ("outputLocation", "Where it is written"),
        ],
        _ => &[],
    }
}

fn entries(context: &str, settings: bool) -> &'static [Entry] {
    match context {
        "" if settings => &SETTINGS,
        "" => &TOP,
        "plugins" => &PLUGINS,
        "repositories" => &REPOSITORIES,
        "java" => &JAVA,
        "toolchain" => &TOOLCHAIN,
        "application" => &APPLICATION,
        "jacoco" => &JACOCO,
        "tasks" => &TASKS,
        "test" => &TEST,
        "jacocoTestReport" => &JACOCO_REPORT,
        "reports" => &REPORTS,
        "task" | "javadoc" | "compileJava" => &TASK,
        "jar" => &JAR,
        "maven" => &MAVEN_REPO,
        "buildscript" => &BUILDSCRIPT,
        "allprojects" | "subprojects" => &PROJECTS,
        "sourceSets" => &SOURCE_SETS,
        "main" => &SOURCE_SET,
        "pluginManagement" => &PLUGIN_MANAGEMENT,
        "dependencyResolutionManagement" => &RESOLUTION_MANAGEMENT,
        _ => &[],
    }
}

/// A string the cursor is in: where its text starts and ends (bytes).
#[derive(Debug, Clone, Copy)]
struct Str {
    start: usize,
    end: usize,
}

/// Where the cursor is in a Gradle script.
#[derive(Debug, Clone)]
struct Place {
    /// Block names from the outside in (`dependencies`, `java`, `toolchain`).
    blocks: Vec<String>,
    string: Option<Str>,
    /// The identifier being typed: byte where it starts.
    word_start: usize,
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'`' || c == b'-'
}

/// Skip back over a balanced `(…)` or `<…>` ending at `end` (exclusive); returns its start.
fn skip_balanced(b: &[u8], end: usize, open: u8, close: u8) -> usize {
    let mut depth = 0i32;
    let mut i = end;
    while i > 0 {
        i -= 1;
        if b[i] == close {
            depth += 1;
        } else if b[i] == open {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
    }
    0
}

/// The name of the block opened by the `{` at `brace`: `toolchain`, `register` (with its
/// arguments), `tasks.test` → `test`.
fn block_name(b: &[u8], brace: usize) -> String {
    let mut i = brace;
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let mut args = String::new();
    let mut type_arg = String::new();
    if i > 0 && b[i - 1] == b')' {
        let s = skip_balanced(b, i, b'(', b')');
        args = String::from_utf8_lossy(&b[s + 1..i - 1]).to_string();
        i = s;
    }
    if i > 0 && b[i - 1] == b'>' {
        let s = skip_balanced(b, i, b'<', b'>');
        type_arg = String::from_utf8_lossy(&b[s + 1..i - 1]).to_string();
        i = s;
    }
    let end = i;
    while i > 0 && (is_ident(b[i - 1]) || b[i - 1] == b'.') {
        i -= 1;
    }
    let chain = String::from_utf8_lossy(&b[i..end])
        .trim_matches('`')
        .to_string();
    let last = chain.rsplit('.').next().unwrap_or("").to_string();
    match last.as_str() {
        "register" | "named" | "withType" | "create" | "getByName" | "configureEach" => {
            let a = format!("{args} {type_arg}");
            if a.contains("Test") || a.contains("\"test\"") || a.contains("'test'") {
                "test".into()
            } else if a.contains("JacocoReport") || a.contains("jacocoTestReport") {
                "jacocoTestReport".into()
            } else if a.contains("Jar") || a.contains("\"jar\"") || a.contains("'jar'") {
                "jar".into()
            } else {
                "task".into()
            }
        }
        _ => last,
    }
}

fn place(text: &str, pos: usize) -> Place {
    let clean = blank_comments(text);
    let b = clean.as_bytes();
    let pos = pos.min(b.len());
    let mut blocks: Vec<String> = Vec::new();
    let mut quote: Option<(u8, usize)> = None;
    let mut i = 0;
    while i < pos {
        let c = b[i];
        if let Some((q, _)) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q || c == b'\n' {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some((c, i + 1)),
            b'{' => blocks.push(block_name(b, i)),
            b'}' => {
                blocks.pop();
            }
            _ => {}
        }
        i += 1;
    }
    let string = quote.map(|(q, start)| {
        let end = b[pos..]
            .iter()
            .position(|&c| c == q || c == b'\n')
            .map(|e| pos + e)
            .filter(|&e| b[e] == q)
            .unwrap_or(pos);
        Str { start, end }
    });
    let mut w = pos;
    while w > 0 && (is_ident(b[w - 1])) {
        w -= 1;
    }
    Place {
        blocks,
        string,
        word_start: w,
    }
}

/// The identifier just before `at` (skipping whitespace and an opening parenthesis).
fn callee(b: &[u8], at: usize) -> String {
    let mut i = at;
    while i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
        i -= 1;
    }
    if i > 0 && b[i - 1] == b'(' {
        i -= 1;
    }
    while i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
        i -= 1;
    }
    let end = i;
    while i > 0 && is_ident(b[i - 1]) {
        i -= 1;
    }
    String::from_utf8_lossy(&b[i..end]).to_string()
}

fn is_configuration(name: &str) -> bool {
    CONFIGURATIONS.iter().any(|(c, _)| *c == name)
        || matches!(
            name,
            "platform" | "enforcedPlatform" | "classpath" | "kapt" | "ksp"
        )
        || name.ends_with("Implementation")
        || name.ends_with("CompileOnly")
        || name.ends_with("RuntimeOnly")
}

/// What a string at the cursor completes.
enum StringKind {
    Coordinates,
    PluginId,
    Kotlin,
    Other,
}

fn string_kind(text: &str, p: &Place, s: Str) -> StringKind {
    let b = text.as_bytes();
    let call = callee(b, s.start - 1);
    let context = p.blocks.last().map(String::as_str).unwrap_or("");
    if context == "plugins" && call == "id" {
        StringKind::PluginId
    } else if context == "plugins" && call == "kotlin" {
        StringKind::Kotlin
    } else if is_configuration(&call) && p.blocks.iter().any(|b| b == "dependencies") {
        StringKind::Coordinates
    } else {
        StringKind::Other
    }
}

fn coordinates_request(prefix: &str) -> Request {
    let parts: Vec<&str> = prefix.split(':').collect();
    if parts.len() >= 3 {
        Request::Versions {
            group: parts[0].to_string(),
            artifact: parts[1].to_string(),
            prefix: parts[2..].join(":"),
        }
    } else {
        Request::Coordinates {
            prefix: prefix.to_string(),
        }
    }
}

/// The text of the XML element the cursor is in (`groupId`, …), its start and end.
fn pom_element(text: &str, pos: usize) -> Option<(String, usize, usize)> {
    let before = &text[..pos];
    let lt = before.rfind('<')?;
    let gt = before[lt..].find('>')? + lt;
    let tag = &before[lt + 1..gt];
    if tag.starts_with('/') || tag.contains(' ') || tag.ends_with('/') {
        return None;
    }
    let end = text[pos..].find('<').map_or(pos, |e| pos + e);
    Some((tag.to_string(), gt + 1, end))
}

/// The `<dependency>`, `<plugin>` or `<parent>` block around the cursor.
fn pom_block(text: &str, pos: usize) -> Option<&str> {
    let before = &text[..pos];
    let (start, name) = ["dependency", "plugin", "parent", "extension"]
        .iter()
        .filter_map(|n| before.rfind(&format!("<{n}>")).map(|i| (i, *n)))
        .max_by_key(|(i, _)| *i)?;
    let close = format!("</{name}>");
    if before[start..].contains(&close) {
        return None;
    }
    let end = text[start..].find(&close).map_or(text.len(), |e| start + e);
    Some(&text[start..end])
}

fn pom_value(block: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find('<')? + s;
    let v = block[s..e].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Where the cursor is in a pom: the element and what Maven Central must answer.
fn pom_request(text: &str, pos: usize) -> Option<(String, usize, usize, Request)> {
    let (tag, start, end) = pom_element(text, pos)?;
    let block = pom_block(text, pos)?;
    let prefix = text[start..pos].trim_start().to_string();
    // The element's own text does not count as "known".
    let without = block.replacen(
        &format!("<{tag}>{}", &text[start..end]),
        &format!("<{tag}>"),
        1,
    );
    let group = pom_value(&without, "groupId");
    let artifact = pom_value(&without, "artifactId");
    let req = match tag.as_str() {
        "groupId" => Request::Coordinates { prefix },
        "artifactId" => match &group {
            Some(g) => Request::Coordinates {
                prefix: format!("{g}:{prefix}"),
            },
            None => Request::Coordinates { prefix },
        },
        "version" => Request::Versions {
            group: group?,
            artifact: artifact?,
            prefix,
        },
        _ => return None,
    };
    Some((tag, start, end, req))
}

/// What Maven Central must answer for the completion at UTF-16 `offset` (`None`: nothing,
/// the DSL model answers alone).
pub fn request(script: Script, text: &str, offset: usize) -> Option<Request> {
    let pos = byte_offset(text, offset);
    if script == Script::Pom {
        return pom_request(text, pos).map(|r| r.3);
    }
    let p = place(text, pos);
    let s = p.string?;
    match string_kind(text, &p, s) {
        StringKind::Coordinates => Some(coordinates_request(&text[s.start..pos])),
        _ => None,
    }
}

fn item(label: &str, kind: &'static str, detail: &str, insert: &str) -> Item {
    Item {
        label: label.to_string(),
        kind,
        detail: (!detail.is_empty()).then(|| detail.to_string()),
        documentation: None,
        insert: insert.to_string(),
        required: false,
        deprecated: false,
        retrigger: false,
    }
}

/// Artifacts for a coordinate prefix: Maven Central's, else the well-known ones.
fn artifacts(prefix: &str, data: &CentralData) -> Vec<Artifact> {
    let mut out: Vec<Artifact> = data.artifacts.clone();
    if out.is_empty() {
        let p = prefix.to_ascii_lowercase();
        out = POPULAR
            .iter()
            .filter(|(g, a, _)| {
                let id = format!("{g}:{a}");
                match p.split_once(':') {
                    Some((pg, pa)) => *g == pg && a.starts_with(pa),
                    None => id.contains(&p),
                }
            })
            .map(|(g, a, v)| Artifact {
                group: g.to_string(),
                artifact: a.to_string(),
                latest_version: v.to_string(),
            })
            .collect();
    }
    if let Some((g, a)) = prefix.split_once(':') {
        out.retain(|x| x.group == g && x.artifact.starts_with(a));
    }
    out
}

fn version_items(data: &CentralData) -> Vec<Item> {
    let Some(v) = &data.versions else {
        return Vec::new();
    };
    v.versions
        .iter()
        .take(60)
        .map(|ver| {
            let detail = if Some(ver) == v.latest.as_ref() {
                "latest release"
            } else if version::is_prerelease(ver) {
                "pre-release"
            } else {
                ""
            };
            item(ver, "value", detail, ver)
        })
        .collect()
}

fn out(
    text: &str,
    from: usize,
    replace: (usize, usize),
    items: Vec<Item>,
) -> Option<CompletionOut> {
    if items.is_empty() {
        return None;
    }
    let u = Utf16::new(text);
    Some(CompletionOut {
        from: u.of(from),
        replace_from: u.of(replace.0),
        replace_to: u.of(replace.1),
        items,
    })
}

fn complete_pom(text: &str, pos: usize, data: &CentralData) -> Option<CompletionOut> {
    let (tag, start, end, req) = pom_request(text, pos)?;
    let lead = text[start..pos].len() - text[start..pos].trim_start().len();
    let from = start + lead;
    let trail = text[from..end].trim_end().len();
    let replace = (from, from + trail);
    let items = match (&req, tag.as_str()) {
        (Request::Versions { .. }, _) => version_items(data),
        (Request::Coordinates { prefix }, "groupId") => {
            let mut groups: Vec<Item> = Vec::new();
            for a in artifacts(prefix, data) {
                if !groups.iter().any(|g| g.label == a.group) {
                    groups.push(item(&a.group, "value", "", &a.group));
                }
            }
            groups
        }
        (Request::Coordinates { prefix }, _) => artifacts(prefix, data)
            .into_iter()
            .map(|a| {
                item(
                    &a.artifact,
                    "value",
                    &format!("{} · {}", a.group, a.latest_version),
                    &a.artifact,
                )
            })
            .collect(),
    };
    out(text, from, replace, items)
}

/// Completion at UTF-16 `offset`; `data` is Maven Central's answer to [`request`] (empty
/// when there was none).
pub fn complete(
    script: Script,
    text: &str,
    offset: usize,
    data: &CentralData,
) -> Option<CompletionOut> {
    let pos = byte_offset(text, offset);
    let (kotlin, settings) = match script {
        Script::Pom => return complete_pom(text, pos, data),
        Script::Kotlin { settings } => (true, settings),
        Script::Groovy { settings } => (false, settings),
    };
    let p = place(text, pos);
    if let Some(s) = p.string {
        let replace = (s.start, s.end.max(pos));
        let prefix = &text[s.start..pos];
        let items = match string_kind(text, &p, s) {
            StringKind::Coordinates => match coordinates_request(prefix) {
                Request::Versions {
                    group, artifact, ..
                } => {
                    let from = s.start + group.len() + artifact.len() + 2;
                    return out(text, from, (from, replace.1), version_items(data));
                }
                Request::Coordinates { prefix } => artifacts(&prefix, data)
                    .into_iter()
                    .map(|a| {
                        let id = format!("{}:{}", a.group, a.artifact);
                        let insert = if a.latest_version.is_empty() {
                            format!("{id}:$1")
                        } else {
                            format!("{id}:{}", a.latest_version)
                        };
                        item(&id, "value", &a.latest_version, &insert)
                    })
                    .collect(),
            },
            StringKind::PluginId => PLUGIN_IDS
                .iter()
                .map(|(id, d)| item(id, "value", d, id))
                .collect(),
            StringKind::Kotlin => [
                ("jvm", "Kotlin for the JVM"),
                ("plugin.spring", "Kotlin Spring support"),
                ("kapt", "Annotation processing"),
            ]
            .iter()
            .map(|(id, d)| item(id, "value", d, id))
            .collect(),
            StringKind::Other => return None,
        };
        return out(text, s.start, replace, items);
    }
    let b = text.as_bytes();
    let word_start = p.word_start;
    // `receiver.` members.
    if word_start > 0 && b[word_start - 1] == b'.' {
        let mut r = word_start - 1;
        while r > 0 && is_ident(b[r - 1]) {
            r -= 1;
        }
        let receiver = &text[r..word_start - 1];
        let items: Vec<Item> = members(receiver)
            .iter()
            .map(|(name, detail)| {
                let insert = match (receiver, *name) {
                    ("JavaLanguageVersion", "of") => "of($1)".to_string(),
                    ("tasks", "register") if kotlin => {
                        "register<$1>(\"$2\") {\n\t$0\n}".to_string()
                    }
                    ("tasks", "register") => "register('$2', $1) {\n\t$0\n}".to_string(),
                    ("tasks", "named") if kotlin => "named(\"$1\") {\n\t$0\n}".to_string(),
                    ("tasks", "named") => "named('$1') {\n\t$0\n}".to_string(),
                    ("tasks", "withType") if kotlin => "withType<$1> {\n\t$0\n}".to_string(),
                    ("tasks", "withType") => "withType($1) {\n\t$0\n}".to_string(),
                    ("tasks", _) => format!("{name} {{\n\t$0\n}}"),
                    _ => name.to_string(),
                };
                item(name, "property", detail, &insert)
            })
            .collect();
        return out(text, word_start, (word_start, pos), items);
    }
    let context = p.blocks.last().map(String::as_str).unwrap_or("");
    let mut items: Vec<Item> = Vec::new();
    if context == "dependencies" {
        for (name, detail) in CONFIGURATIONS {
            let insert = if kotlin {
                format!("{name}(\"$1\")")
            } else {
                format!("{name} '$1'")
            };
            let mut i = item(name, "property", detail, &insert);
            i.retrigger = true;
            items.push(i);
        }
        for (label, detail, kts, groovy) in DEPENDENCY_HELPERS {
            items.push(item(
                label,
                "snippet",
                detail,
                if kotlin { kts } else { groovy },
            ));
        }
    } else {
        for (label, detail, kts, groovy) in entries(context, settings) {
            let kind = if kts.contains('{') {
                "snippet"
            } else {
                "property"
            };
            items.push(item(label, kind, detail, if kotlin { kts } else { groovy }));
        }
    }
    out(text, word_start, (word_start, pos), items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::central::{parse_metadata, parse_search};

    const KTS: Script = Script::Kotlin { settings: false };
    const GROOVY: Script = Script::Groovy { settings: false };

    /// Text with `|` as the cursor → (text, UTF-16 offset).
    fn at(s: &str) -> (String, usize) {
        let i = s.find('|').unwrap();
        (s.replacen('|', "", 1), s[..i].encode_utf16().count())
    }

    fn labels(script: Script, s: &str, data: &CentralData) -> Vec<String> {
        let (text, off) = at(s);
        complete(script, &text, off, data)
            .map(|c| c.items.into_iter().map(|i| i.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn blocks_choose_the_items() {
        let none = CentralData::default();
        let top = labels(KTS, "plugins { java }\nde|", &none);
        assert!(
            top.contains(&"dependencies".to_string()) && top.contains(&"repositories".to_string())
        );
        let deps = labels(KTS, "dependencies {\n    te|\n}", &none);
        assert!(deps.contains(&"testImplementation".to_string()));
        assert!(deps.contains(&"platform".to_string()));
        let toolchain = labels(KTS, "java {\n  toolchain {\n    |\n  }\n}", &none);
        assert_eq!(toolchain, ["languageVersion", "vendor", "implementation"]);
        let test = labels(
            KTS,
            "tasks.register<Test>(\"integrationTest\") {\n  |\n}",
            &none,
        );
        assert!(test.contains(&"useJUnitPlatform".to_string()));
        let test = labels(KTS, "tasks.test {\n  use|\n}", &none);
        assert!(test.contains(&"useJUnitPlatform".to_string()));
        let custom = labels(KTS, "tasks.register<Copy>(\"bundle\") {\n  |\n}", &none);
        assert!(custom.contains(&"doLast".to_string()));
        let app = labels(GROOVY, "application {\n  // { not a block\n  m|\n}", &none);
        assert!(app.contains(&"mainClass".to_string()));
        let repos = labels(KTS, "repositories {\n  m|\n}", &none);
        assert_eq!(repos[0], "mavenCentral");
        let settings = labels(Script::Kotlin { settings: true }, "|", &none);
        assert!(settings.contains(&"rootProject.name".to_string()));
        let after_close = labels(KTS, "java {\n}\n|", &none);
        assert!(after_close.contains(&"plugins".to_string()));
    }

    #[test]
    fn insert_texts_follow_the_dsl() {
        let none = CentralData::default();
        let (t, o) = at("dependencies {\n    imp|\n}");
        let c = complete(KTS, &t, o, &none).unwrap();
        let i = c
            .items
            .iter()
            .find(|i| i.label == "implementation")
            .unwrap();
        assert_eq!(i.insert, "implementation(\"$1\")");
        assert!(i.retrigger);
        assert_eq!(c.from, 19);
        let c = complete(GROOVY, &t, o, &none).unwrap();
        assert_eq!(
            c.items
                .iter()
                .find(|i| i.label == "implementation")
                .unwrap()
                .insert,
            "implementation '$1'"
        );
        let (t, o) = at("plugins {\n  |\n}");
        let c = complete(GROOVY, &t, o, &none).unwrap();
        assert_eq!(c.items[0].insert, "id 'java'");
    }

    #[test]
    fn members_after_a_dot() {
        let none = CentralData::default();
        assert!(labels(KTS, "tasks.re|", &none).contains(&"register".to_string()));
        let vendors = labels(KTS, "java { toolchain { vendor = JvmVendorSpec.|", &none);
        assert!(vendors.contains(&"ADOPTIUM".to_string()));
        assert!(labels(KTS, "foo.|", &none).is_empty());
    }

    #[test]
    fn coordinates_and_versions_in_dependency_strings() {
        let (t, o) = at("dependencies {\n    implementation(\"guav|\")\n}");
        assert_eq!(
            request(KTS, &t, o),
            Some(Request::Coordinates {
                prefix: "guav".into()
            })
        );
        let data = CentralData {
            artifacts: parse_search(crate::central::fixtures::SEARCH).unwrap(),
            versions: None,
        };
        let c = complete(KTS, &t, o, &data).unwrap();
        assert_eq!(c.items[0].label, "com.google.guava:guava");
        assert_eq!(c.items[0].insert, "com.google.guava:guava:33.3.1-jre");
        assert_eq!(c.replace_from, 35);
        assert_eq!(c.replace_to, 39);
        // Offline: the well-known artifacts.
        let c = complete(KTS, &t, o, &CentralData::default()).unwrap();
        assert_eq!(c.items[0].label, "com.google.guava:guava");
        // Groovy strings and BOMs.
        let (t, o) = at("dependencies {\n  testImplementation platform('org.junit:junit-b|')\n}");
        assert_eq!(
            request(GROOVY, &t, o),
            Some(Request::Coordinates {
                prefix: "org.junit:junit-b".into()
            })
        );
        let c = complete(GROOVY, &t, o, &CentralData::default()).unwrap();
        assert_eq!(c.items[0].label, "org.junit:junit-bom");

        let (t, o) = at("dependencies {\n  implementation(\"com.google.guava:guava:3|\")\n}");
        assert_eq!(
            request(KTS, &t, o),
            Some(Request::Versions {
                group: "com.google.guava".into(),
                artifact: "guava".into(),
                prefix: "3".into()
            })
        );
        let data = CentralData {
            artifacts: vec![],
            versions: Some(parse_metadata(crate::central::fixtures::METADATA).unwrap()),
        };
        let c = complete(KTS, &t, o, &data).unwrap();
        assert!(c.items[0].label.starts_with("33.4.0"));
        assert_eq!(
            &t[..c.from],
            "dependencies {\n  implementation(\"com.google.guava:guava:"
        );
        // Not in dependencies: no lookup.
        let (t, o) = at("application { mainClass = \"com.|\" }");
        assert_eq!(request(KTS, &t, o), None);
        assert!(complete(KTS, &t, o, &CentralData::default()).is_none());
    }

    #[test]
    fn plugin_ids() {
        let none = CentralData::default();
        let ids = labels(KTS, "plugins {\n  id(\"org.spr|\")\n}", &none);
        assert!(ids.contains(&"org.springframework.boot".to_string()));
        let ids = labels(GROOVY, "plugins {\n  id 'ja|'\n}", &none);
        assert!(ids.contains(&"java-library".to_string()));
    }

    #[test]
    fn pom_coordinates() {
        let pom = "<project><dependencies>\n<dependency>\n  <groupId>org.junit.jupiter</groupId>\n  <artifactId>junit-jupiter-a|</artifactId>\n</dependency></dependencies></project>";
        let (t, o) = at(pom);
        assert_eq!(
            request(Script::Pom, &t, o),
            Some(Request::Coordinates {
                prefix: "org.junit.jupiter:junit-jupiter-a".into()
            })
        );
        let data = CentralData {
            artifacts: parse_search(crate::central::fixtures::GROUP).unwrap(),
            versions: None,
        };
        let c = complete(Script::Pom, &t, o, &data).unwrap();
        assert_eq!(
            c.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["junit-jupiter-api"]
        );
        let pom = "<dependency>\n  <groupId>com.google.guava</groupId>\n  <artifactId>guava</artifactId>\n  <version>|</version>\n</dependency>";
        let (t, o) = at(pom);
        assert_eq!(
            request(Script::Pom, &t, o),
            Some(Request::Versions {
                group: "com.google.guava".into(),
                artifact: "guava".into(),
                prefix: String::new()
            })
        );
        let data = CentralData {
            artifacts: vec![],
            versions: Some(parse_metadata(crate::central::fixtures::METADATA).unwrap()),
        };
        let c = complete(Script::Pom, &t, o, &data).unwrap();
        assert_eq!(c.items.len(), 8);
        assert_eq!(
            c.items
                .iter()
                .filter(|i| i.detail.as_deref() == Some("latest release"))
                .count(),
            1
        );
        let (t, o) = at("<dependency><groupId>com.goo|</groupId></dependency>");
        let c = complete(Script::Pom, &t, o, &CentralData::default()).unwrap();
        assert_eq!(c.items[0].label, "com.google.guava");
        assert_eq!(
            request(Script::Pom, "<project><name>x</name></project>", 16),
            None
        );
        assert_eq!(
            Script::of_path("a/settings.gradle.kts"),
            Some(Script::Kotlin { settings: true })
        );
    }
}
