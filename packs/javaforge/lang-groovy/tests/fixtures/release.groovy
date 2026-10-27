#!/usr/bin/env groovy
// Bumps the version name in app/build.gradle and prints the release notes.
import groovy.transform.Field

@Field String buildFile = 'app/build.gradle'

def version = args ? args[0] : '1.4.0'
def gradle = new File(buildFile)
gradle.text = gradle.text.replaceAll(/versionName "[^"]+"/, "versionName \"$version\"")

def notes(String v) {
    "Release $v: offline sync, faster search"
}

println "Bumped ${buildFile} to $version"
println notes(version)
