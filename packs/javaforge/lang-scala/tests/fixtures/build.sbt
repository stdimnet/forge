// The shop: a core library, an HTTP API and an aggregating root.
val scala3Version = "3.3.4"
val catsVersion = "2.12.0"

ThisBuild / organization := "com.acme"
ThisBuild / version      := "0.4.0-SNAPSHOT"
ThisBuild / scalaVersion := scala3Version

lazy val commonSettings = Seq(
  libraryDependencies += "org.scalameta" %% "munit" % "1.0.2" % Test
)

lazy val root = (project in file("."))
  .aggregate(core, api)
  .settings(
    name := "shop",
    publish / skip := true
  )

lazy val core = project
  .settings(
    commonSettings,
    name := "shop-core",
    libraryDependencies ++= Seq(
      "org.typelevel" %% "cats-core" % catsVersion,
      "org.scalatest" %% "scalatest" % "3.2.19" % "test"
    )
  )

lazy val api = project.in(file("modules/api"))
  .dependsOn(core % "compile->compile;test->test")
  .settings(
    commonSettings,
    name := "shop-api",
    scalaVersion := "2.13.15",
    libraryDependencies += "com.softwaremill.sttp.tapir" %% "tapir-core" % "1.11.7",
    libraryDependencies += "ch.qos.logback" % "logback-classic" % "1.5.12" % Runtime
  )
