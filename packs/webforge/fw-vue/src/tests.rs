use super::*;
use webforge_lang_web::text::byte_to_utf16;
use webforge_lang_web::SourceFile;

const CARD: &str = r#"<script setup lang="ts">
import { computed } from 'vue'
import type { Product } from '@/types'

interface Props {
  product: Product
  compact?: boolean
  badge?: string | null
}

const props = withDefaults(defineProps<Props>(), { compact: false })
const emit = defineEmits<{
  (e: 'add-to-cart', id: string): void
  (e: 'favorite'): void
}>()
const quantity = defineModel<number>('quantity')
const price = computed(() => props.product.price.toFixed(2))
</script>

<template>
  <article class="card" :class="{ compact }">
    <h3>{{ product.name }}</h3>
    <BaseButton @click="emit('add-to-cart', product.id)">Add</BaseButton>
  </article>
</template>

<style scoped>
.card { padding: 1rem; }
</style>
"#;

const LIST: &str = r#"<template>
  <section>
    <template v-for="p in products" :key="p.id">
      <ProductCard :product="p" compact @add-to-cart="add" v-model:quantity="q" />
    </template>
    <base-button>More</base-button>
    <router-link to="/cart">Cart</router-link>
  </section>
</template>

<script setup lang="ts">
import ProductCard from './ProductCard.vue'
import { BaseButton } from '@/components/ui'
defineProps(['products', 'loading'])
</script>
"#;

const BUTTON: &str = r#"<script>
export default {
  name: 'BaseButton',
  props: {
    variant: { type: String, default: 'primary' },
    disabled: { type: Boolean, required: true },
    size: String,
  },
  emits: ['click'],
}
</script>

<template><button :class="variant"><slot /></button></template>
"#;

fn sources() -> Sources {
    let f = |path: &str, content: &str| SourceFile {
        path: path.into(),
        content: content.into(),
    };
    Sources::new(
        vec![
            f("src/components/ProductCard.vue", CARD),
            f("src/components/ProductList.vue", LIST),
            f("src/components/ui/BaseButton.vue", BUTTON),
            f("src/components/ui/index.ts", "export { default as BaseButton } from './BaseButton.vue'\n"),
            f("src/components/Counter.ts", "import { defineComponent } from 'vue'\nexport default defineComponent({\n  name: 'click-counter',\n  props: ['start'],\n})\n"),
        ],
        [],
    )
}

fn at(content: &str, needle: &str, delta: usize) -> usize {
    byte_to_utf16(content, content.find(needle).unwrap() + delta)
}

#[test]
fn parses_blocks() {
    let sfc = Sfc::parse(LIST);
    let t = sfc.template.unwrap();
    assert!(LIST[t.start..t.end].contains("<template v-for"));
    assert!(LIST[t.start..t.end].trim_end().ends_with("</section>"));
    assert_eq!(sfc.scripts.len(), 1);
    assert!(sfc.scripts[0].setup);
    assert_eq!(sfc.scripts[0].lang.as_deref(), Some("ts"));
    assert_eq!(Sfc::parse(CARD).styles.len(), 1);
}

#[test]
fn script_setup_props_emits_and_models() {
    let c = scan_file("src/components/ProductCard.vue", CARD).remove(0);
    assert_eq!((c.name.as_str(), c.selector.as_deref()), ("ProductCard", Some("product-card")));
    let props: Vec<_> = c.props.iter().map(|p| (p.name.as_str(), p.required)).collect();
    assert_eq!(props, [("product", true), ("compact", false), ("badge", false), ("quantity", false)]);
    assert_eq!(c.props[0].location.line, 5);
    let events: Vec<_> = c.events.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(events, ["add-to-cart", "favorite"]);
    assert_eq!(c.template.as_ref().unwrap().line, 19);
    assert_eq!(c.styles.len(), 1);
}

#[test]
fn options_api_and_runtime_declarations() {
    let c = scan_file("src/components/ui/BaseButton.vue", BUTTON).remove(0);
    let props: Vec<_> = c
        .props
        .iter()
        .map(|p| (p.name.as_str(), p.type_text.as_deref().unwrap_or(""), p.required))
        .collect();
    assert_eq!(props, [("variant", "String", false), ("disabled", "Boolean", true), ("size", "String", false)]);
    assert_eq!(c.events[0].name, "click");

    let list = scan_file("src/components/ProductList.vue", LIST).remove(0);
    let names: Vec<_> = list.props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["products", "loading"]);

    let counter = scan_file("src/components/Counter.ts", "export default defineComponent({\n  name: 'click-counter',\n  props: ['start'],\n})\n");
    assert_eq!((counter[0].name.as_str(), counter[0].kind), ("ClickCounter", ComponentKind::Script));
}

#[test]
fn goto_component_from_template_tags() {
    let s = sources();
    let index = VueIndex::new(&s);
    let card = index.goto("src/components/ProductList.vue", LIST, at(LIST, "<ProductCard", 4));
    assert_eq!(card, [Location { path: "src/components/ProductCard.vue".into(), line: 0, column: 0 }]);
    // Kebab-case tag through the barrel's `default as BaseButton` re-export.
    let button = index.goto("src/components/ProductList.vue", LIST, at(LIST, "<base-button", 3));
    assert_eq!(button[0].path, "src/components/ui/BaseButton.vue");
    // Not imported in ProductCard: found project-wide.
    let global = index.goto("src/components/ProductCard.vue", CARD, at(CARD, "<BaseButton", 3));
    assert_eq!(global[0].path, "src/components/ui/BaseButton.vue");
    // Unknown and built-in tags.
    assert!(index.goto("src/components/ProductList.vue", LIST, at(LIST, "<router-link", 3)).is_empty());
    assert!(index.goto("src/components/ProductList.vue", LIST, at(LIST, "<section", 3)).is_empty());
}

#[test]
fn goto_props_and_emits_from_attributes() {
    let s = sources();
    let index = VueIndex::new(&s);
    let goto = |needle: &str, delta: usize| index.goto("src/components/ProductList.vue", LIST, at(LIST, needle, delta));
    assert_eq!(goto(":product=", 3)[0].line, 5);
    assert_eq!(goto("compact @", 2)[0].line, 6);
    let event = goto("@add-to-cart", 4);
    assert_eq!((event[0].path.as_str(), event[0].line), ("src/components/ProductCard.vue", 12));
    let model = goto("v-model:quantity", 10);
    assert_eq!(model[0].line, 15);
}

#[test]
fn usages_and_counts() {
    let s = sources();
    let index = VueIndex::new(&s);
    let counts: Vec<_> = index
        .components()
        .into_iter()
        .map(|c| (c.name, c.usages))
        .collect();
    assert!(counts.contains(&("ProductCard".to_string(), 1)));
    assert!(counts.contains(&("BaseButton".to_string(), 2)));
    let usages = index.goto("src/components/ui/BaseButton.vue", BUTTON, 3);
    assert_eq!(usages.len(), 2);
}

#[test]
fn nuxt_auto_import_names() {
    assert_eq!(nuxt_name("components/base/Button.vue").as_deref(), Some("BaseButton"));
    assert_eq!(nuxt_name("components/base/BaseInput.vue").as_deref(), Some("BaseInput"));
    assert_eq!(nuxt_name("pages/index.vue"), None);
}
