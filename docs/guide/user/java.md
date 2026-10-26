---
title: JavaForge
description: The JavaForge pack - Java with Eclipse JDT LS, JDK detection and Temurin downloads, Maven and Gradle, Application runs, JUnit 5/4 and TestNG with JaCoCo coverage, and the Java debugger (java-debug).
order: 115
tags: [java, javaforge, jdk, maven, gradle, junit, testng, jdtls, debugger]
---

# JavaForge

**JavaForge** (plugin `forge.java`) brings Java to Forge in the style of IntelliJ IDEA. It
activates in [Java projects](../getting-started/project-detection.md): a `pom.xml`, a Gradle
build (`build.gradle`, `build.gradle.kts`, `settings.gradle`, `settings.gradle.kts`), `*.java`
files in a folder or a `src/main/java` / `src/test/java` tree. Maven and Gradle are reported as
the project's tools.

![The Java editor with completion from Eclipse JDT LS](../../screenshots/java-editor-completion.png)

## Eclipse JDT Language Server

Code intelligence comes from [Eclipse JDT LS](https://github.com/eclipse-jdtls/eclipse.jdt.ls)
(EPL-2.0) through Forge's shared language server client: completion, Go to Declaration and
Implementation, Find Usages, hover with Javadoc, parameter info, diagnostics in the editor and
the Problems tool window, Rename, quick fixes and code actions (`Alt+Enter`), Optimize Imports
(`Ctrl+Alt+O`) and Reformat Code (`Ctrl+Alt+L`).

JDT LS is never bundled with Forge. When a Java project opens without it, a balloon offers
**Install**, **Not now** and **Don't ask again** (setting **Install Eclipse JDT LS**: Ask,
Always, Never). Install downloads, with your consent:

- the newest JDT LS milestone (or snapshot, see **Release channel**) from
  `download.eclipse.org/jdtls`, checked against the `.sha256` file published next to it, into
  `<data dir>/forge/langservers/jdtls/<version>`;
- Microsoft's [java-debug](https://github.com/microsoft/java-debug) plugin (EPL-1.0) from
  Maven Central, checked against the repository's checksum, into
  `<data dir>/forge/langservers/java-debug`.

Progress shows in the status bar. **Install Eclipse JDT Language Server** in Find Action
(`Ctrl+Shift+A`) installs or updates both at any time.

![The install offer](../../screenshots/java-jdtls-install.png)

Forge starts JDT LS itself, like the official `jdtls` launcher: with a JDK that it can run on
(Java 21 for 2024–2025 builds; newer builds need Java 25, which Forge reads from the bundles of
the installation), the installation read-only, and its OSGi configuration and the workspace data
in the user cache directory (`<cache dir>/forge/jdtls`), never inside the project. Every
detected JDK is announced to JDT LS as a runtime, the project JDK as the default.

While JDT LS imports a Maven or Gradle build, the status bar shows its progress ("Importing
Gradle project(s) 45%"). After editing a build file, run **Reload All Maven and Gradle Projects**
(Find Action, the main menu's Build menu or the Gradle / Maven tool window) to re-import it.

Settings | Languages & Frameworks | **Java** shows the installed versions and the JDK JDT LS runs
on, and has these options:

| Setting | |
| --- | --- |
| Install Eclipse JDT LS | Ask, Always or Never when a Java project opens without it |
| JDT LS installation | Use your own unpacked JDT LS (a folder with `plugins/` and `config_*`) |
| JDK for the language server | The JDK that runs JDT LS; empty: the project JDK when it is new enough, else the newest that is |
| VM options | `-Xmx1G` by default |
| Release channel | Milestones or snapshots |
| Download site | `https://download.eclipse.org/jdtls` |
| Hot code replace | Automatically, on request or never (see [Debugging](#debugging)) |
| java-debug repository | `https://repo1.maven.org/maven2` |

![Settings | Languages & Frameworks | Java](../../screenshots/java-settings-jdtls.png)

JDT LS imports and builds projects, which runs code of the project (Gradle build scripts,
annotation processors), so it does not start in [Safe Mode](project-trust.md). It runs for local
projects; remote projects do not start it yet.

## JDKs

Settings | Build, Execution, Deployment | **JDKs** combines IntelliJ's SDK list and Project
settings:

- **Project JDK**: the JDK the project builds and runs with (empty: the newest). Runs of Maven
  and Gradle get it as `JAVA_HOME`.
- **Language level**: the Java release of the sources; empty means the build's (toolchain,
  `maven.compiler.release`) or the project JDK's.
- The JDK list: every JDK Forge detects, with its name (`temurin-21`), home, version, vendor and
  where it was found. Detection reads each JDK's `release` file and looks at `JAVA_HOME`, `java`
  on `PATH`, `/usr/lib/jvm` and `/usr/java` (Linux), `/Library/Java/JavaVirtualMachines` and
  `~/Library/Java/JavaVirtualMachines` (macOS), `Program Files` and the registry (Windows),
  SDKMAN! (`~/.sdkman/candidates/java`), asdf, jabba, Gradle toolchains (`~/.gradle/jdks`),
  IntelliJ's `~/.jdks` and Forge's own downloads. A JRE without `javac` is not listed.
- **+** adds a JDK from disk, **Detect Again** rescans.

![Settings | Build, Execution, Deployment | JDKs](../../screenshots/java-jdks-settings.png)

**Download JDK…** (the download button, or Find Action) downloads Eclipse Temurin from the
[Adoptium API](https://api.adoptium.net) (setting **Adoptium API**): pick a version (LTS releases
are marked), and Forge downloads the build for your platform, checks it against the SHA-256 the
API publishes and unpacks it into `<data dir>/forge/jdks/<release>`.

![Download JDK](../../screenshots/java-jdk-download.png)

## Maven and Gradle

Forge reads the build in the project root: the tool (with both a `pom.xml` and a Gradle build,
setting **Build tool of projects with both**), the wrapper (`mvnw`, `gradlew`), the Kotlin or
Groovy DSL, modules (`include` in `settings.gradle(.kts)`, `<modules>` in `pom.xml`), JaCoCo,
the `application` plugin and its main class, Spring Boot, custom tasks and the Java release.

Runs use the wrapper when the project has one (settings **Use the Gradle wrapper** / **Use the
Maven wrapper**); otherwise the configured **Gradle executable** / **Maven home path**, or
`gradle` / `mvn` on `PATH`. **Offline work** adds `--offline` / `-o`. These options live in
Settings | Build, Execution, Deployment | Build Tools | Maven and Gradle.

- **Build Project** (`Ctrl+F9`) compiles main and test sources through the build tool
  (Gradle `classes testClasses`, Maven `test-compile`).
- The **Gradle** and **Maven** tool windows (right stripe) list the build's tasks by group, or
  Maven's lifecycle phases and plugin goals. Double-click runs one; the toolbar reloads the
  project.

![The Gradle tool window](../../screenshots/java-gradle-tool-window.png)

## Run configurations

| Type | |
| --- | --- |
| **Application** | Runs a main class with the configuration's JDK (empty: the project JDK), VM options, program arguments, environment and working directory. The class path comes from the build through JDT LS (java-debug resolves it after JDT LS compiled the project); without JDT LS it is the module's build output (`build/classes/java/main`, `target/classes`). **Class path** and **Java module** (`-m module/class`) override it. |
| **Gradle** | Runs tasks (`clean build`), optionally in a module (`:app:build`). |
| **Maven** | Runs goals (`clean install -DskipTests`), optionally in a module (`-pl app -am`). |
| **JUnit**, **TestNG** | Runs all tests, a package, a class, a method or a list of tests through the build tool; see [Tests](#tests). |
| **Remote JVM Debug** | Attaches the debugger to a JVM started with `-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005`. |

A green ▶ in the gutter next to a `main` method (`public static void main(String[] args)`, and
Java 25's instance `void main()` and compact source files) offers Run, Debug and Modify Run
Configuration; `Ctrl+Shift+F10` runs the class at the caret.

![The Run gutter on main](../../screenshots/java-run-gutter.png)

## Tests

JUnit 5 (`@Test`, `@ParameterizedTest`, `@RepeatedTest`, `@TestFactory`, `@TestTemplate`,
`@Nested` classes), JUnit 4 (`@Test`), JUnit 3 (`TestCase` subclasses with `test*` methods) and
TestNG (method and class level `@Test`) get gutter icons on their classes and methods, chosen by
the file's imports. They run on the [shared test runner](testing.md) through the build tool, as
IntelliJ delegates to Gradle and Maven:

- Gradle: `cleanTest test --tests 'com.acme.CartTest.adds'` (so up-to-date tests run again);
- Maven Surefire: `test -Dtest='com.acme.CartTest#adds' -Dsurefire.failIfNoSpecifiedTests=false`.

The results come from the JUnit XML reports the build writes (`build/test-results/test`,
`target/surefire-reports`), which Forge empties before the run. The tree links tests and stack
traces to their sources; **Rerun Failed Tests** runs the failed ones only.

![A JUnit 5 run through Gradle](../../screenshots/java-junit-tree.png)

**Run with Coverage** works when the build configures JaCoCo: Gradle runs `jacocoTestReport`
(enable its XML report: `reports { xml.required = true }`), Maven runs
`org.jacoco:jacoco-maven-plugin:report` (with `prepare-agent` bound in the build). Forge reads
`build/reports/jacoco/test/jacocoTestReport.xml` or `target/site/jacoco/jacoco.xml` into the
[Coverage](coverage.md) tool window and the editor gutter.

## Debugging

Debug (`Shift+F9`) on an Application, Gradle (`run` and friends), JUnit or TestNG configuration
starts the JVM with a JDWP agent that waits for the debugger (Application: a free port;
Gradle: `--debug-jvm` on port 5005; Maven Surefire: `-Dmaven.surefire.debug`), and attaches
java-debug as soon as the JVM listens. java-debug runs inside JDT LS and serves the Debug
Adapter Protocol, so the [Debug tool window](debugging.md) works as for every other language:
threads, frames, variables with expandable objects, watches, Evaluate Expression, breakpoints
with conditions and logpoints, exception breakpoints (caught and uncaught) and inline values.

**Remote JVM Debug** attaches to a JVM on another machine or in a container at `host:port`.

**Hot code replace**: with **Automatically** (the default), java-debug reloads changed classes
into the running JVM after JDT LS rebuilt them on save.

![A java-debug session](../../screenshots/java-debug-session.png)

## Browser mode

`pnpm dev` with `?project=java` opens the demo Gradle service `inventory-service`
(`?project=java&jdtls=missing` starts without JDT LS to try the install offer).
