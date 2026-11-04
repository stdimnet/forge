/**
 * `?project=minecraft`: a Paper plugin as the Minecraft Plugin wizard generates it (Gradle
 * Kotlin DSL, run-paper, plugin.yml), plus a chat listener. The descriptor check, Ctrl+B
 * on `main:` and the event handler gutter icons go through the Rust `javaforge-minecraft`
 * crate (WebAssembly, minecraftMock.ts); plugin.yml completion and validation through the
 * bundled JSON schema.
 */
import type { ProjectInfo } from "@/ipc/project";

/** True when the page asks for the Minecraft plugin demo. */
export const isMinecraftDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "minecraft";

export const minecraftDemoName = "better-chat";

const paper = { id: "minecraft", name: "Paper Plugin", version: "1.21.8", language: "java" };

export const minecraftDemoInfo: ProjectInfo = {
  languages: ["java"],
  frameworks: [paper],
  packs: ["javaforge"],
  tools: ["git", "gradle"],
  primary: paper,
};

/** The file the demo opens first. */
export const minecraftDemoStartFile = "src/main/resources/plugin.yml";

export const minecraftDemoFiles: Record<string, string> = {
  ".forge/runConfigurations.toml": `selected = "Run Paper Server"

[[configuration]]
name = "Run Paper Server"
type = "gradle"

[configuration.options]
tasks = "runServer"
`,
  ".gitignore": `# Gradle and Maven
.gradle/
build/
target/

# The test server of run-paper (runServer)
run/

# IDEs
.idea/
*.iml
out/
.vscode/
.DS_Store
`,
  "README.md": `# BetterChat

Nicer chat messages

A Paper plugin for Minecraft 1.21.8, written in Java (Java 21).

## Build

The project has no Gradle wrapper yet. Create it once with an installed Gradle:

\`\`\`sh
gradle wrapper
\`\`\`

Then build the plugin:

\`\`\`sh
./gradlew build
\`\`\`

The plugin jar is \`build/libs/better-chat-1.0.0-SNAPSHOT.jar\`. Copy it into the \`plugins/\` folder of your server.

## Test server

\`./gradlew runServer\` (or the **Run Paper Server** run configuration in Forge) downloads Paper 1.21.8 with [run-paper](https://github.com/jpenilla/run-gradle), builds the plugin and starts a server in \`run/\` with the plugin installed. Accept the Minecraft EULA in \`run/eula.txt\` on the first start.

## Structure

- \`src/main/java/com/example/betterchat/BetterChat.java\`: the main class
- \`src/main/java/com/example/betterchat/command/HelloCommand.java\`: the \`/hello\` command
- \`src/main/java/com/example/betterchat/listener/JoinListener.java\`: greets players when they join
- \`src/main/resources/plugin.yml\`: the plugin descriptor
`,
  "build.gradle.kts": `plugins {
    java
    id("xyz.jpenilla.run-paper") version "2.3.1"
}

group = "com.example"
version = "1.0.0-SNAPSHOT"
description = "Nicer chat messages"

repositories {
    mavenCentral()
    maven("https://repo.papermc.io/repository/maven-public/")
}

dependencies {
    compileOnly("io.papermc.paper:paper-api:1.21.8-R0.1-SNAPSHOT")
}

java {
    toolchain.languageVersion.set(JavaLanguageVersion.of(21))
}

tasks {
    compileJava {
        options.encoding = "UTF-8"
        options.release.set(21)
    }
    processResources {
        val props = mapOf("version" to version)
        inputs.properties(props)
        filteringCharset = "UTF-8"
        filesMatching("plugin.yml") {
            expand(props)
        }
    }
    runServer {
        // The Paper version of the test server (./run).
        minecraftVersion("1.21.8")
    }
}
`,
  "settings.gradle.kts": `rootProject.name = "better-chat"
`,
  "src/main/java/com/example/betterchat/BetterChat.java": `package com.example.betterchat;

import com.example.betterchat.command.HelloCommand;
import com.example.betterchat.listener.ChatListener;
import com.example.betterchat.listener.JoinListener;
import org.bukkit.command.PluginCommand;
import org.bukkit.plugin.java.JavaPlugin;

public final class BetterChat extends JavaPlugin {

    @Override
    public void onEnable() {
        getServer().getPluginManager().registerEvents(new JoinListener(), this);
        getServer().getPluginManager().registerEvents(new ChatListener(), this);
        PluginCommand hello = getCommand("hello");
        if (hello != null) {
            hello.setExecutor(new HelloCommand());
        }
        getLogger().info("BetterChat enabled");
    }

    @Override
    public void onDisable() {
        getLogger().info("BetterChat disabled");
    }
}
`,
  "src/main/java/com/example/betterchat/command/HelloCommand.java": `package com.example.betterchat.command;

import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;

/** {@code /hello}: greets whoever runs it. */
public final class HelloCommand implements CommandExecutor {

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        sender.sendMessage(Component.text("Hello, " + sender.getName() + "!", NamedTextColor.GREEN));
        return true;
    }
}
`,
  "src/main/java/com/example/betterchat/listener/ChatListener.java": `package com.example.betterchat.listener;

import io.papermc.paper.event.player.AsyncChatEvent;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;

/** Colors chat messages and announces broken diamond ores. */
public final class ChatListener implements Listener {

    @EventHandler(priority = EventPriority.HIGH, ignoreCancelled = true)
    public void onChat(AsyncChatEvent event) {
        event.message(event.message().color(NamedTextColor.AQUA));
    }

    @EventHandler
    public void onBlockBreak(BlockBreakEvent event) {
        if (event.getBlock().getType().name().endsWith("DIAMOND_ORE")) {
            event.getPlayer().sendMessage(Component.text("Lucky you!", NamedTextColor.GOLD));
        }
    }
}
`,
  "src/main/java/com/example/betterchat/listener/JoinListener.java": `package com.example.betterchat.listener;

import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;

public final class JoinListener implements Listener {

    @EventHandler
    public void onPlayerJoin(PlayerJoinEvent event) {
        String name = event.getPlayer().getName();
        event.getPlayer().sendMessage(Component.text("Welcome, " + name + "!", NamedTextColor.YELLOW));
    }
}
`,
  "src/main/resources/config.yml": `# BetterChat configuration
chat-color: aqua
announce-diamonds: true
`,
  "src/main/resources/plugin.yml": `name: BetterChat
version: '\${version}'
main: com.example.betterchat.BetterChat
api-version: '1.21'
description: 'Nicer chat messages'
authors: ['Alex', 'Sam']
website: 'https://example.com'

commands:
  hello:
    description: Says hello
    usage: /<command>
`,
};
