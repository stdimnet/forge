//! Source templates of generated mods: the main class, the client / server initializers, an
//! example item and block, mixins and the data generator, in Java (Fabric, NeoForge) and
//! Kotlin (Fabric). Placeholders are `{{name}}` keys filled by [`fill`]; the names of
//! Minecraft's classes come from [`Api`] (Mojang's names or Yarn, per version).

use std::collections::BTreeMap;

/// The names of Minecraft classes and members a template uses.
#[derive(Debug, Clone)]
pub struct Api {
    pub identifier_import: &'static str,
    pub identifier: &'static str,
    /// `Identifier.fromNamespaceAndPath`, `ResourceLocation.fromNamespaceAndPath`, `Identifier.of`.
    pub identifier_of: &'static str,
    pub registry_import: &'static str,
    pub builtin_import: &'static str,
    pub builtin: &'static str,
    pub keys_import: &'static str,
    pub keys: &'static str,
    pub key_import: &'static str,
    pub key: &'static str,
    pub key_of: &'static str,
    pub item_import: &'static str,
    pub block_item_import: &'static str,
    pub item_props: &'static str,
    pub block_import: &'static str,
    pub block_props_import: &'static str,
    pub block_props: &'static str,
    pub block_props_new: &'static str,
    pub sound_import: &'static str,
    pub sound: &'static str,
    pub set_id: &'static str,
    pub block_prefix: &'static str,
    pub load_level: &'static str,
    pub client_import: &'static str,
    pub client: &'static str,
}

impl Api {
    /// Mojang's names; `identifier` is `Identifier` from 1.21.11 on, `ResourceLocation` before.
    pub fn mojang(identifier: bool) -> Api {
        let (identifier_import, ident, identifier_of) = if identifier {
            (
                "net.minecraft.resources.Identifier",
                "Identifier",
                "Identifier.fromNamespaceAndPath",
            )
        } else {
            (
                "net.minecraft.resources.ResourceLocation",
                "ResourceLocation",
                "ResourceLocation.fromNamespaceAndPath",
            )
        };
        Api {
            identifier_import,
            identifier: ident,
            identifier_of,
            registry_import: "net.minecraft.core.Registry",
            builtin_import: "net.minecraft.core.registries.BuiltInRegistries",
            builtin: "BuiltInRegistries",
            keys_import: "net.minecraft.core.registries.Registries",
            keys: "Registries",
            key_import: "net.minecraft.resources.ResourceKey",
            key: "ResourceKey",
            key_of: "ResourceKey.create",
            item_import: "net.minecraft.world.item.Item",
            block_item_import: "net.minecraft.world.item.BlockItem",
            item_props: "Item.Properties",
            block_import: "net.minecraft.world.level.block.Block",
            block_props_import: "net.minecraft.world.level.block.state.BlockBehaviour",
            block_props: "BlockBehaviour.Properties",
            block_props_new: "BlockBehaviour.Properties.of()",
            sound_import: "net.minecraft.world.level.block.SoundType",
            sound: "sound(SoundType.STONE)",
            set_id: "setId",
            block_prefix: "useBlockDescriptionPrefix()",
            load_level: "loadLevel",
            client_import: "net.minecraft.client.Minecraft",
            client: "Minecraft",
        }
    }

    /// Fabric's Yarn names.
    pub fn yarn() -> Api {
        Api {
            identifier_import: "net.minecraft.util.Identifier",
            identifier: "Identifier",
            identifier_of: "Identifier.of",
            registry_import: "net.minecraft.registry.Registry",
            builtin_import: "net.minecraft.registry.Registries",
            builtin: "Registries",
            keys_import: "net.minecraft.registry.RegistryKeys",
            keys: "RegistryKeys",
            key_import: "net.minecraft.registry.RegistryKey",
            key: "RegistryKey",
            key_of: "RegistryKey.of",
            item_import: "net.minecraft.item.Item",
            block_item_import: "net.minecraft.item.BlockItem",
            item_props: "Item.Settings",
            block_import: "net.minecraft.block.Block",
            block_props_import: "net.minecraft.block.AbstractBlock",
            block_props: "AbstractBlock.Settings",
            block_props_new: "AbstractBlock.Settings.create()",
            sound_import: "net.minecraft.sound.BlockSoundGroup",
            sound: "sounds(BlockSoundGroup.STONE)",
            set_id: "registryKey",
            block_prefix: "useBlockPrefixedTranslationKey()",
            load_level: "loadWorld",
            client_import: "net.minecraft.client.MinecraftClient",
            client: "MinecraftClient",
        }
    }

    fn vars(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("identifier_import", self.identifier_import),
            ("identifier", self.identifier),
            ("identifier_of", self.identifier_of),
            ("registry_import", self.registry_import),
            ("builtin_import", self.builtin_import),
            ("builtin", self.builtin),
            ("keys_import", self.keys_import),
            ("keys", self.keys),
            ("key_import", self.key_import),
            ("key", self.key),
            ("key_of", self.key_of),
            ("item_import", self.item_import),
            ("block_item_import", self.block_item_import),
            ("item_props", self.item_props),
            ("block_import", self.block_import),
            ("block_props_import", self.block_props_import),
            ("block_props", self.block_props),
            ("block_props_new", self.block_props_new),
            ("sound_import", self.sound_import),
            ("sound", self.sound),
            ("set_id", self.set_id),
            ("block_prefix", self.block_prefix),
            ("load_level", self.load_level),
            ("client_import", self.client_import),
            ("client", self.client),
        ]
    }
}

/// Replace `{{key}}` placeholders; `api` adds the Minecraft names.
pub fn fill(template: &str, vars: &BTreeMap<&str, String>, api: Option<&Api>) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    if let Some(api) = api {
        for (k, v) in api.vars() {
            out = out.replace(&format!("{{{{{k}}}}}"), v);
        }
    }
    out
}

// ---- Fabric, Java -----------------------------------------------------------------------------

pub const FABRIC_MAIN: &str = r#"package {{package}};

import net.fabricmc.api.ModInitializer;
import {{identifier_import}};
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import {{package}}.registry.ModBlocks;
import {{package}}.registry.ModItems;

public class {{class}} implements ModInitializer {
    public static final String MOD_ID = "{{modid}}";

    // Named after the mod id, so the log shows which mod wrote a line.
    public static final Logger LOGGER = LoggerFactory.getLogger(MOD_ID);

    @Override
    public void onInitialize() {
        // Runs as soon as Minecraft is ready to load mods: register content here.
        ModItems.initialize();
        ModBlocks.initialize();
        LOGGER.info("Hello from {{name}}!");
    }

    public static {{identifier}} id(String path) {
        return {{identifier_of}}(MOD_ID, path);
    }
}
"#;

pub const FABRIC_CLIENT: &str = r#"package {{package}}.client;

import net.fabricmc.api.ClientModInitializer;
import {{package}}.{{class}};

public class {{class}}Client implements ClientModInitializer {
    @Override
    public void onInitializeClient() {
        // Client-only setup: renderers, key bindings, screens.
        {{class}}.LOGGER.info("Hello from the client of {{name}}!");
    }
}
"#;

pub const FABRIC_SERVER: &str = r#"package {{package}};

import net.fabricmc.api.DedicatedServerModInitializer;

public class {{class}}Server implements DedicatedServerModInitializer {
    @Override
    public void onInitializeServer() {
        // Runs only on dedicated servers.
        {{class}}.LOGGER.info("Hello from the server of {{name}}!");
    }
}
"#;

/// `{{register_item}}`: the body of `register` (with or without registry keys).
pub const FABRIC_ITEMS: &str = r#"package {{package}}.registry;

import java.util.function.Function;
{{item_imports}}
import {{package}}.{{class}};

public final class ModItems {
    public static final Item EXAMPLE_ITEM = register("example_item", Item::new, new {{item_props}}());

    private ModItems() {
    }

    public static <T extends Item> T register(String name, Function<{{item_props}}, T> factory, {{item_props}} properties) {
{{register_item}}
    }

    /** Loads the class, which registers its items. */
    public static void initialize() {
    }
}
"#;

pub const FABRIC_BLOCKS: &str = r#"package {{package}}.registry;

import java.util.function.Function;
{{block_imports}}
import {{package}}.{{class}};

public final class ModBlocks {
    public static final Block EXAMPLE_BLOCK = register("example_block", Block::new,
            {{block_props_new}}.strength(1.5f).{{sound}});

    private ModBlocks() {
    }

    /** Registers the block and its item. */
    public static Block register(String name, Function<{{block_props}}, Block> factory, {{block_props}} properties) {
{{register_block}}
        ModItems.register(name, p -> new BlockItem(block, p), new {{item_props}}(){{block_item_prefix}});
        return block;
    }

    /** Loads the class, which registers its blocks. */
    public static void initialize() {
    }
}
"#;

pub const FABRIC_MIXIN: &str = r#"package {{package}}.mixin;

import net.minecraft.server.MinecraftServer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(MinecraftServer.class)
public class ExampleMixin {
    @Inject(at = @At("HEAD"), method = "{{load_level}}")
    private void init(CallbackInfo info) {
        // Injected at the start of MinecraftServer.{{load_level}}().
    }
}
"#;

/// `{{client_mixin_package}}`: `<package>.client.mixin` (Fabric) or `<package>.mixin.client`.
pub const CLIENT_MIXIN: &str = r#"package {{client_mixin_package}};

import {{client_import}};
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin({{client}}.class)
public class ExampleClientMixin {
    @Inject(at = @At("HEAD"), method = "run")
    private void init(CallbackInfo info) {
        // Injected at the start of {{client}}.run().
    }
}
"#;

pub const FABRIC_DATAGEN: &str = r#"package {{datagen_package}};

import net.fabricmc.fabric.api.datagen.v1.DataGeneratorEntrypoint;
import net.fabricmc.fabric.api.datagen.v1.FabricDataGenerator;

public class {{class}}DataGenerator implements DataGeneratorEntrypoint {
    @Override
    public void onInitializeDataGenerator(FabricDataGenerator generator) {
        FabricDataGenerator.Pack pack = generator.createPack();
        // Add providers (models, recipes, loot tables, tags, translations) to the pack:
        // pack.addProvider(MyRecipeProvider::new);
    }
}
"#;

// ---- Fabric, Kotlin ---------------------------------------------------------------------------

pub const FABRIC_KT_MAIN: &str = r#"package {{package}}

import net.fabricmc.api.ModInitializer
import {{identifier_import}}
import org.slf4j.Logger
import org.slf4j.LoggerFactory
import {{package}}.registry.ModBlocks
import {{package}}.registry.ModItems

class {{class}} : ModInitializer {
    override fun onInitialize() {
        // Runs as soon as Minecraft is ready to load mods: register content here.
        ModItems.initialize()
        ModBlocks.initialize()
        LOGGER.info("Hello from {{name}}!")
    }

    companion object {
        const val MOD_ID = "{{modid}}"

        // Named after the mod id, so the log shows which mod wrote a line.
        @JvmField
        val LOGGER: Logger = LoggerFactory.getLogger(MOD_ID)

        fun id(path: String): {{identifier}} = {{identifier_of}}(MOD_ID, path)
    }
}
"#;

pub const FABRIC_KT_CLIENT: &str = r#"package {{package}}.client

import net.fabricmc.api.ClientModInitializer
import {{package}}.{{class}}

class {{class}}Client : ClientModInitializer {
    override fun onInitializeClient() {
        // Client-only setup: renderers, key bindings, screens.
        {{class}}.LOGGER.info("Hello from the client of {{name}}!")
    }
}
"#;

pub const FABRIC_KT_SERVER: &str = r#"package {{package}}

import net.fabricmc.api.DedicatedServerModInitializer

class {{class}}Server : DedicatedServerModInitializer {
    override fun onInitializeServer() {
        // Runs only on dedicated servers.
        {{class}}.LOGGER.info("Hello from the server of {{name}}!")
    }
}
"#;

pub const FABRIC_KT_ITEMS: &str = r#"package {{package}}.registry

{{item_imports}}
import {{package}}.{{class}}

object ModItems {
    val EXAMPLE_ITEM: Item = register("example_item", ::Item, {{item_props}}())

    fun <T : Item> register(name: String, factory: ({{item_props}}) -> T, properties: {{item_props}}): T {
{{register_item}}
    }

    /** Loads the object, which registers its items. */
    fun initialize() {}
}
"#;

pub const FABRIC_KT_BLOCKS: &str = r#"package {{package}}.registry

{{block_imports}}
import {{package}}.{{class}}

object ModBlocks {
    val EXAMPLE_BLOCK: Block = register("example_block", ::Block, {{block_props_new}}.strength(1.5f).{{sound}})

    /** Registers the block and its item. */
    fun register(name: String, factory: ({{block_props}}) -> Block, properties: {{block_props}}): Block {
{{register_block}}
        ModItems.register(name, { p -> BlockItem(block, p) }, {{item_props}}(){{block_item_prefix}})
        return block
    }

    /** Loads the object, which registers its blocks. */
    fun initialize() {}
}
"#;

pub const FABRIC_KT_DATAGEN: &str = r#"package {{datagen_package}}

import net.fabricmc.fabric.api.datagen.v1.DataGeneratorEntrypoint
import net.fabricmc.fabric.api.datagen.v1.FabricDataGenerator

class {{class}}DataGenerator : DataGeneratorEntrypoint {
    override fun onInitializeDataGenerator(generator: FabricDataGenerator) {
        val pack = generator.createPack()
        // Add providers (models, recipes, loot tables, tags, translations) to the pack:
        // pack.addProvider(::MyRecipeProvider)
    }
}
"#;

// ---- NeoForge ---------------------------------------------------------------------------------

pub const NEO_MAIN: &str = r#"package {{package}};

import org.slf4j.Logger;

import com.mojang.logging.LogUtils;

import net.minecraft.world.item.CreativeModeTabs;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.event.BuildCreativeModeTabContentsEvent;
{{extra_imports}}import {{package}}.registry.ModBlocks;
import {{package}}.registry.ModItems;

// The value must match the modId in META-INF/neoforge.mods.toml.
@Mod({{class}}.MOD_ID)
public class {{class}} {
    public static final String MOD_ID = "{{modid}}";
    public static final Logger LOGGER = LogUtils.getLogger();

    // The first code that runs when the mod loads. NeoForge passes the mod's event bus and
    // container.
    public {{class}}(IEventBus modEventBus, ModContainer modContainer) {
        ModBlocks.BLOCKS.register(modEventBus);
        ModItems.ITEMS.register(modEventBus);
        modEventBus.addListener(this::addCreative);
{{extra_listeners}}        LOGGER.info("Hello from {{name}}!");
    }

    // Adds the example item and block to the creative inventory.
    private void addCreative(BuildCreativeModeTabContentsEvent event) {
        if (event.getTabKey() == CreativeModeTabs.INGREDIENTS) {
            event.accept(ModItems.EXAMPLE_ITEM);
        }
        if (event.getTabKey() == CreativeModeTabs.BUILDING_BLOCKS) {
            event.accept(ModBlocks.EXAMPLE_BLOCK_ITEM);
        }
    }
}
"#;

/// `{{dist}}`: `CLIENT` or `DEDICATED_SERVER`; `{{setup_event}}` and its import.
pub const NEO_SIDE: &str = r#"package {{package}};

import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.{{setup_event}};

// Loaded only on the {{side}}.
@Mod(value = {{class}}.MOD_ID, dist = Dist.{{dist}})
public class {{class}}{{suffix}} {
    public {{class}}{{suffix}}(IEventBus modEventBus, ModContainer container) {
        modEventBus.addListener(this::setup);
    }

    private void setup({{setup_event}} event) {
        {{class}}.LOGGER.info("Hello from the {{side}} of {{name}}!");
    }
}
"#;

pub const NEO_ITEMS: &str = r#"package {{package}}.registry;

import net.minecraft.world.item.Item;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;
import {{package}}.{{class}};

public final class ModItems {
    public static final DeferredRegister.Items ITEMS = DeferredRegister.createItems({{class}}.MOD_ID);

    public static final DeferredItem<Item> EXAMPLE_ITEM = ITEMS.registerSimpleItem("example_item");

    private ModItems() {
    }
}
"#;

pub const NEO_BLOCKS: &str = r#"package {{package}}.registry;

import net.minecraft.world.item.BlockItem;
import net.minecraft.world.level.block.Block;
{{props_import}}import net.minecraft.world.level.material.MapColor;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;
import {{package}}.{{class}};

public final class ModBlocks {
    public static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks({{class}}.MOD_ID);

    public static final DeferredBlock<Block> EXAMPLE_BLOCK = BLOCKS.registerSimpleBlock("example_block",
            {{block_properties}});
    public static final DeferredItem<BlockItem> EXAMPLE_BLOCK_ITEM =
            ModItems.ITEMS.registerSimpleBlockItem("example_block", EXAMPLE_BLOCK);

    private ModBlocks() {
    }
}
"#;

pub const NEO_MIXIN: &str = FABRIC_MIXIN;

/// `{{event}}`: `GatherDataEvent.Client` (1.21.4+) or `GatherDataEvent`.
pub const NEO_DATAGEN: &str = r#"package {{package}}.datagen;

import net.neoforged.neoforge.data.event.GatherDataEvent;

/** Data generation (the runData task writes to src/generated/resources). */
public final class ModDataGenerators {
    private ModDataGenerators() {
    }

    public static void gatherData({{event}} event) {
        // Add providers (models, recipes, loot tables, tags, translations) here, for example
        // event.createProvider(MyRecipeProvider.Runner::new);
    }
}
"#;
