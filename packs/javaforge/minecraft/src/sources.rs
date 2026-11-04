//! Source templates of generated plugins: the main class, a `/hello` command and a join
//! listener, in Java and Kotlin, for each API flavor. Placeholders: `{{package}}`,
//! `{{class}}`, `{{name}}` (the plugin name).

/// The API a plugin's sources are written against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// Paper and forks with `plugin.yml`: Bukkit commands, Adventure messages.
    Paper,
    /// Paper and forks with `paper-plugin.yml`: Brigadier commands through the lifecycle API.
    PaperPlugin,
    /// Spigot and Bukkit: Bukkit commands, `ChatColor` messages.
    Legacy,
    Velocity,
    Bungee,
}

pub struct Sources {
    pub main: &'static str,
    pub command: &'static str,
    pub listener: &'static str,
}

pub fn java(flavor: Flavor) -> Sources {
    match flavor {
        Flavor::Paper => Sources {
            main: JAVA_BUKKIT_MAIN,
            command: JAVA_PAPER_COMMAND,
            listener: JAVA_PAPER_LISTENER,
        },
        Flavor::PaperPlugin => Sources {
            main: JAVA_PAPER_PLUGIN_MAIN,
            command: JAVA_BASIC_COMMAND,
            listener: JAVA_PAPER_LISTENER,
        },
        Flavor::Legacy => Sources {
            main: JAVA_BUKKIT_MAIN,
            command: JAVA_LEGACY_COMMAND,
            listener: JAVA_LEGACY_LISTENER,
        },
        Flavor::Velocity => Sources {
            main: JAVA_VELOCITY_MAIN,
            command: JAVA_VELOCITY_COMMAND,
            listener: JAVA_VELOCITY_LISTENER,
        },
        Flavor::Bungee => Sources {
            main: JAVA_BUNGEE_MAIN,
            command: JAVA_BUNGEE_COMMAND,
            listener: JAVA_BUNGEE_LISTENER,
        },
    }
}

pub fn kotlin(flavor: Flavor) -> Sources {
    match flavor {
        Flavor::Paper => Sources {
            main: KT_BUKKIT_MAIN,
            command: KT_PAPER_COMMAND,
            listener: KT_PAPER_LISTENER,
        },
        Flavor::PaperPlugin => Sources {
            main: KT_PAPER_PLUGIN_MAIN,
            command: KT_BASIC_COMMAND,
            listener: KT_PAPER_LISTENER,
        },
        Flavor::Legacy => Sources {
            main: KT_BUKKIT_MAIN,
            command: KT_LEGACY_COMMAND,
            listener: KT_LEGACY_LISTENER,
        },
        Flavor::Velocity => Sources {
            main: KT_VELOCITY_MAIN,
            command: KT_VELOCITY_COMMAND,
            listener: KT_VELOCITY_LISTENER,
        },
        Flavor::Bungee => Sources {
            main: KT_BUNGEE_MAIN,
            command: KT_BUNGEE_COMMAND,
            listener: KT_BUNGEE_LISTENER,
        },
    }
}

// ---- Java -----------------------------------------------------------------------------------

const JAVA_BUKKIT_MAIN: &str = r#"package {{package}};

import {{package}}.command.HelloCommand;
import {{package}}.listener.JoinListener;
import org.bukkit.command.PluginCommand;
import org.bukkit.plugin.java.JavaPlugin;

public final class {{class}} extends JavaPlugin {

    @Override
    public void onEnable() {
        getServer().getPluginManager().registerEvents(new JoinListener(), this);
        PluginCommand hello = getCommand("hello");
        if (hello != null) {
            hello.setExecutor(new HelloCommand());
        }
        getLogger().info("{{name}} enabled");
    }

    @Override
    public void onDisable() {
        getLogger().info("{{name}} disabled");
    }
}
"#;

const JAVA_PAPER_PLUGIN_MAIN: &str = r#"package {{package}};

import {{package}}.command.HelloCommand;
import {{package}}.listener.JoinListener;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import org.bukkit.plugin.java.JavaPlugin;

public final class {{class}} extends JavaPlugin {

    @Override
    public void onEnable() {
        getServer().getPluginManager().registerEvents(new JoinListener(), this);
        // paper-plugin.yml has no commands section: commands are registered with Brigadier.
        getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event ->
                event.registrar().register("hello", "Says hello", new HelloCommand()));
        getLogger().info("{{name}} enabled");
    }

    @Override
    public void onDisable() {
        getLogger().info("{{name}} disabled");
    }
}
"#;

const JAVA_PAPER_COMMAND: &str = r#"package {{package}}.command;

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
"#;

const JAVA_BASIC_COMMAND: &str = r#"package {{package}}.command;

import io.papermc.paper.command.brigadier.BasicCommand;
import io.papermc.paper.command.brigadier.CommandSourceStack;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;

/** {@code /hello}: greets whoever runs it. */
public final class HelloCommand implements BasicCommand {

    @Override
    public void execute(CommandSourceStack source, String[] args) {
        String name = source.getSender().getName();
        source.getSender().sendMessage(Component.text("Hello, " + name + "!", NamedTextColor.GREEN));
    }
}
"#;

const JAVA_PAPER_LISTENER: &str = r#"package {{package}}.listener;

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
"#;

const JAVA_LEGACY_COMMAND: &str = r#"package {{package}}.command;

import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;

/** {@code /hello}: greets whoever runs it. */
public final class HelloCommand implements CommandExecutor {

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        sender.sendMessage(ChatColor.GREEN + "Hello, " + sender.getName() + "!");
        return true;
    }
}
"#;

const JAVA_LEGACY_LISTENER: &str = r#"package {{package}}.listener;

import org.bukkit.ChatColor;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;

public final class JoinListener implements Listener {

    @EventHandler
    public void onPlayerJoin(PlayerJoinEvent event) {
        event.getPlayer().sendMessage(ChatColor.YELLOW + "Welcome, " + event.getPlayer().getName() + "!");
    }
}
"#;

const JAVA_VELOCITY_MAIN: &str = r#"package {{package}};

import {{package}}.command.HelloCommand;
import {{package}}.listener.JoinListener;
import com.google.inject.Inject;
import com.velocitypowered.api.command.CommandManager;
import com.velocitypowered.api.event.Subscribe;
import com.velocitypowered.api.event.proxy.ProxyInitializeEvent;
import com.velocitypowered.api.event.proxy.ProxyShutdownEvent;
import com.velocitypowered.api.proxy.ProxyServer;
import org.slf4j.Logger;

public final class {{class}} {

    private final ProxyServer server;
    private final Logger logger;

    @Inject
    public {{class}}(ProxyServer server, Logger logger) {
        this.server = server;
        this.logger = logger;
    }

    @Subscribe
    public void onProxyInitialize(ProxyInitializeEvent event) {
        server.getEventManager().register(this, new JoinListener());
        CommandManager commands = server.getCommandManager();
        commands.register(commands.metaBuilder("hello").plugin(this).build(), new HelloCommand());
        logger.info("{{name}} enabled");
    }

    @Subscribe
    public void onProxyShutdown(ProxyShutdownEvent event) {
        logger.info("{{name}} disabled");
    }
}
"#;

const JAVA_VELOCITY_COMMAND: &str = r#"package {{package}}.command;

import com.velocitypowered.api.command.SimpleCommand;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;

/** {@code /hello}: greets whoever runs it. */
public final class HelloCommand implements SimpleCommand {

    @Override
    public void execute(Invocation invocation) {
        invocation.source().sendMessage(Component.text("Hello from the proxy!", NamedTextColor.GREEN));
    }
}
"#;

const JAVA_VELOCITY_LISTENER: &str = r#"package {{package}}.listener;

import com.velocitypowered.api.event.Subscribe;
import com.velocitypowered.api.event.connection.PostLoginEvent;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;

public final class JoinListener {

    @Subscribe
    public void onPostLogin(PostLoginEvent event) {
        String name = event.getPlayer().getUsername();
        event.getPlayer().sendMessage(Component.text("Welcome, " + name + "!", NamedTextColor.YELLOW));
    }
}
"#;

const JAVA_BUNGEE_MAIN: &str = r#"package {{package}};

import {{package}}.command.HelloCommand;
import {{package}}.listener.JoinListener;
import net.md_5.bungee.api.plugin.Plugin;

public final class {{class}} extends Plugin {

    @Override
    public void onEnable() {
        getProxy().getPluginManager().registerListener(this, new JoinListener());
        getProxy().getPluginManager().registerCommand(this, new HelloCommand());
        getLogger().info("{{name}} enabled");
    }

    @Override
    public void onDisable() {
        getLogger().info("{{name}} disabled");
    }
}
"#;

const JAVA_BUNGEE_COMMAND: &str = r#"package {{package}}.command;

import net.md_5.bungee.api.ChatColor;
import net.md_5.bungee.api.CommandSender;
import net.md_5.bungee.api.chat.TextComponent;
import net.md_5.bungee.api.plugin.Command;

/** {@code /hello}: greets whoever runs it. */
public final class HelloCommand extends Command {

    public HelloCommand() {
        super("hello");
    }

    @Override
    public void execute(CommandSender sender, String[] args) {
        TextComponent message = new TextComponent("Hello from the proxy!");
        message.setColor(ChatColor.GREEN);
        sender.sendMessage(message);
    }
}
"#;

const JAVA_BUNGEE_LISTENER: &str = r#"package {{package}}.listener;

import net.md_5.bungee.api.chat.TextComponent;
import net.md_5.bungee.api.event.PostLoginEvent;
import net.md_5.bungee.api.plugin.Listener;
import net.md_5.bungee.event.EventHandler;

public final class JoinListener implements Listener {

    @EventHandler
    public void onPostLogin(PostLoginEvent event) {
        event.getPlayer().sendMessage(new TextComponent("Welcome, " + event.getPlayer().getName() + "!"));
    }
}
"#;

// ---- Kotlin ---------------------------------------------------------------------------------

const KT_BUKKIT_MAIN: &str = r#"package {{package}}

import {{package}}.command.HelloCommand
import {{package}}.listener.JoinListener
import org.bukkit.plugin.java.JavaPlugin

class {{class}} : JavaPlugin() {

    override fun onEnable() {
        server.pluginManager.registerEvents(JoinListener(), this)
        getCommand("hello")?.setExecutor(HelloCommand())
        logger.info("{{name}} enabled")
    }

    override fun onDisable() {
        logger.info("{{name}} disabled")
    }
}
"#;

const KT_PAPER_PLUGIN_MAIN: &str = r#"package {{package}}

import {{package}}.command.HelloCommand
import {{package}}.listener.JoinListener
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents
import org.bukkit.plugin.java.JavaPlugin

class {{class}} : JavaPlugin() {

    override fun onEnable() {
        server.pluginManager.registerEvents(JoinListener(), this)
        // paper-plugin.yml has no commands section: commands are registered with Brigadier.
        lifecycleManager.registerEventHandler(LifecycleEvents.COMMANDS) { event ->
            event.registrar().register("hello", "Says hello", HelloCommand())
        }
        logger.info("{{name}} enabled")
    }

    override fun onDisable() {
        logger.info("{{name}} disabled")
    }
}
"#;

const KT_PAPER_COMMAND: &str = r#"package {{package}}.command

import net.kyori.adventure.text.Component
import net.kyori.adventure.text.format.NamedTextColor
import org.bukkit.command.Command
import org.bukkit.command.CommandExecutor
import org.bukkit.command.CommandSender

/** `/hello`: greets whoever runs it. */
class HelloCommand : CommandExecutor {

    override fun onCommand(sender: CommandSender, command: Command, label: String, args: Array<out String>): Boolean {
        sender.sendMessage(Component.text("Hello, ${sender.name}!", NamedTextColor.GREEN))
        return true
    }
}
"#;

const KT_BASIC_COMMAND: &str = r#"package {{package}}.command

import io.papermc.paper.command.brigadier.BasicCommand
import io.papermc.paper.command.brigadier.CommandSourceStack
import net.kyori.adventure.text.Component
import net.kyori.adventure.text.format.NamedTextColor

/** `/hello`: greets whoever runs it. */
class HelloCommand : BasicCommand {

    override fun execute(source: CommandSourceStack, args: Array<out String>) {
        source.sender.sendMessage(Component.text("Hello, ${source.sender.name}!", NamedTextColor.GREEN))
    }
}
"#;

const KT_PAPER_LISTENER: &str = r#"package {{package}}.listener

import net.kyori.adventure.text.Component
import net.kyori.adventure.text.format.NamedTextColor
import org.bukkit.event.EventHandler
import org.bukkit.event.Listener
import org.bukkit.event.player.PlayerJoinEvent

class JoinListener : Listener {

    @EventHandler
    fun onPlayerJoin(event: PlayerJoinEvent) {
        event.player.sendMessage(Component.text("Welcome, ${event.player.name}!", NamedTextColor.YELLOW))
    }
}
"#;

const KT_LEGACY_COMMAND: &str = r#"package {{package}}.command

import org.bukkit.ChatColor
import org.bukkit.command.Command
import org.bukkit.command.CommandExecutor
import org.bukkit.command.CommandSender

/** `/hello`: greets whoever runs it. */
class HelloCommand : CommandExecutor {

    override fun onCommand(sender: CommandSender, command: Command, label: String, args: Array<out String>): Boolean {
        sender.sendMessage("${ChatColor.GREEN}Hello, ${sender.name}!")
        return true
    }
}
"#;

const KT_LEGACY_LISTENER: &str = r#"package {{package}}.listener

import org.bukkit.ChatColor
import org.bukkit.event.EventHandler
import org.bukkit.event.Listener
import org.bukkit.event.player.PlayerJoinEvent

class JoinListener : Listener {

    @EventHandler
    fun onPlayerJoin(event: PlayerJoinEvent) {
        event.player.sendMessage("${ChatColor.YELLOW}Welcome, ${event.player.name}!")
    }
}
"#;

const KT_VELOCITY_MAIN: &str = r#"package {{package}}

import {{package}}.command.HelloCommand
import {{package}}.listener.JoinListener
import com.google.inject.Inject
import com.velocitypowered.api.event.Subscribe
import com.velocitypowered.api.event.proxy.ProxyInitializeEvent
import com.velocitypowered.api.event.proxy.ProxyShutdownEvent
import com.velocitypowered.api.proxy.ProxyServer
import org.slf4j.Logger

class {{class}} @Inject constructor(
    private val server: ProxyServer,
    private val logger: Logger,
) {

    @Subscribe
    fun onProxyInitialize(event: ProxyInitializeEvent) {
        server.eventManager.register(this, JoinListener())
        val commands = server.commandManager
        commands.register(commands.metaBuilder("hello").plugin(this).build(), HelloCommand())
        logger.info("{{name}} enabled")
    }

    @Subscribe
    fun onProxyShutdown(event: ProxyShutdownEvent) {
        logger.info("{{name}} disabled")
    }
}
"#;

const KT_VELOCITY_COMMAND: &str = r#"package {{package}}.command

import com.velocitypowered.api.command.SimpleCommand
import net.kyori.adventure.text.Component
import net.kyori.adventure.text.format.NamedTextColor

/** `/hello`: greets whoever runs it. */
class HelloCommand : SimpleCommand {

    override fun execute(invocation: SimpleCommand.Invocation) {
        invocation.source().sendMessage(Component.text("Hello from the proxy!", NamedTextColor.GREEN))
    }
}
"#;

const KT_VELOCITY_LISTENER: &str = r#"package {{package}}.listener

import com.velocitypowered.api.event.Subscribe
import com.velocitypowered.api.event.connection.PostLoginEvent
import net.kyori.adventure.text.Component
import net.kyori.adventure.text.format.NamedTextColor

class JoinListener {

    @Subscribe
    fun onPostLogin(event: PostLoginEvent) {
        event.player.sendMessage(Component.text("Welcome, ${event.player.username}!", NamedTextColor.YELLOW))
    }
}
"#;

const KT_BUNGEE_MAIN: &str = r#"package {{package}}

import {{package}}.command.HelloCommand
import {{package}}.listener.JoinListener
import net.md_5.bungee.api.plugin.Plugin

class {{class}} : Plugin() {

    override fun onEnable() {
        proxy.pluginManager.registerListener(this, JoinListener())
        proxy.pluginManager.registerCommand(this, HelloCommand())
        logger.info("{{name}} enabled")
    }

    override fun onDisable() {
        logger.info("{{name}} disabled")
    }
}
"#;

const KT_BUNGEE_COMMAND: &str = r#"package {{package}}.command

import net.md_5.bungee.api.ChatColor
import net.md_5.bungee.api.CommandSender
import net.md_5.bungee.api.chat.TextComponent
import net.md_5.bungee.api.plugin.Command

/** `/hello`: greets whoever runs it. */
class HelloCommand : Command("hello") {

    override fun execute(sender: CommandSender, args: Array<out String>) {
        val message = TextComponent("Hello from the proxy!")
        message.color = ChatColor.GREEN
        sender.sendMessage(message)
    }
}
"#;

const KT_BUNGEE_LISTENER: &str = r#"package {{package}}.listener

import net.md_5.bungee.api.chat.TextComponent
import net.md_5.bungee.api.event.PostLoginEvent
import net.md_5.bungee.api.plugin.Listener
import net.md_5.bungee.event.EventHandler

class JoinListener : Listener {

    @EventHandler
    fun onPostLogin(event: PostLoginEvent) {
        event.player.sendMessage(TextComponent("Welcome, ${event.player.name}!"))
    }
}
"#;
