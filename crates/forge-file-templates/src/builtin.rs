//! The templates Forge ships, named and written like PHPStorm's (Settings | Editor | File and
//! Code Templates).

use crate::TemplateKind;

/// A built-in template.
#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    pub kind: TemplateKind,
    pub name: &'static str,
    pub extension: &'static str,
    pub text: &'static str,
    /// Offered only when the project uses this language or framework (`ProjectInfo::uses`).
    pub when: Option<&'static str>,
    /// Menu group: templates of one group stand together in File | New.
    pub group: &'static str,
    /// Shown below the editor in the settings page.
    pub description: &'static str,
}

const fn file(
    name: &'static str,
    extension: &'static str,
    when: Option<&'static str>,
    group: &'static str,
    text: &'static str,
) -> Builtin {
    Builtin {
        kind: TemplateKind::File,
        name,
        extension,
        text,
        when,
        group,
        description: "",
    }
}

macro_rules! php_type {
    ($decl:literal) => {
        concat!(
            "<?php\n#parse(\"PHP File Header.php\")\n\n#if (${NAMESPACE})\nnamespace ${NAMESPACE};\n\n#end\n",
            $decl,
            "\n{\n\n}\n"
        )
    };
}

/// Names of the built-in templates the code uses directly.
pub mod names {
    pub const PHP_FILE_HEADER: &str = "PHP File Header";
    pub const PHP_CLASS: &str = "PHP Class";
    pub const PHP_INTERFACE: &str = "PHP Interface";
    pub const PHP_TRAIT: &str = "PHP Trait";
    pub const PHP_ENUM: &str = "PHP Enum";
    pub const IMPLEMENTED_METHOD_BODY: &str = "PHP Implemented Method Body";
    pub const OVERRIDDEN_METHOD_BODY: &str = "PHP Overridden Method Body";
    pub const CLASS_DOC_COMMENT: &str = "PHP Class Doc Comment";
    pub const FUNCTION_DOC_COMMENT: &str = "PHP Function Doc Comment";
    pub const FIELD_DOC_COMMENT: &str = "PHP Field Doc Comment";
}

pub const BUILTINS: &[Builtin] = &[
    // ---- Files
    file("PHP File", "php", Some("php"), "php", "<?php\n#parse(\"PHP File Header.php\")\n\n"),
    file(
        names::PHP_CLASS,
        "php",
        Some("php"),
        "php",
        php_type!("class ${NAME}#if (${EXTENDS}) extends ${EXTENDS}#end#if (${IMPLEMENTS}) implements ${IMPLEMENTS}#end"),
    ),
    file(
        names::PHP_INTERFACE,
        "php",
        Some("php"),
        "php",
        php_type!("interface ${NAME}#if (${EXTENDS}) extends ${EXTENDS}#end"),
    ),
    file(names::PHP_TRAIT, "php", Some("php"), "php", php_type!("trait ${NAME}")),
    file(
        names::PHP_ENUM,
        "php",
        Some("php"),
        "php",
        php_type!("enum ${NAME}#if (${BACKED_TYPE}): ${BACKED_TYPE}#end#if (${IMPLEMENTS}) implements ${IMPLEMENTS}#end"),
    ),
    file(
        "PHPUnit Test",
        "php",
        Some("php"),
        "php",
        concat!(
            "<?php\n#parse(\"PHP File Header.php\")\n\n#if (${NAMESPACE})\nnamespace ${NAMESPACE};\n\n#end\n",
            "use PHPUnit\\Framework\\TestCase;\n\nclass ${NAME} extends TestCase\n{\n\n}\n"
        ),
    ),
    file(
        "Laravel Controller",
        "php",
        Some("laravel"),
        "framework",
        concat!(
            "<?php\n#parse(\"PHP File Header.php\")\n\n#if (${NAMESPACE})\nnamespace ${NAMESPACE};\n\n#end\n",
            "use Illuminate\\Http\\Request;\n\nclass ${NAME} extends Controller\n{\n    //\n}\n"
        ),
    ),
    file(
        "Laravel Model",
        "php",
        Some("laravel"),
        "framework",
        concat!(
            "<?php\n#parse(\"PHP File Header.php\")\n\n#if (${NAMESPACE})\nnamespace ${NAMESPACE};\n\n#end\n",
            "use Illuminate\\Database\\Eloquent\\Model;\n\nclass ${NAME} extends Model\n{\n    //\n}\n"
        ),
    ),
    file(
        "Symfony Controller",
        "php",
        Some("symfony"),
        "framework",
        concat!(
            "<?php\n#parse(\"PHP File Header.php\")\n\n#if (${NAMESPACE})\nnamespace ${NAMESPACE};\n\n#end\n",
            "#set ($ROUTE = ${NAME.replace(\"Controller\", \"\").toLowerCase()})\n",
            "use Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;\n",
            "use Symfony\\Component\\HttpFoundation\\Response;\n",
            "use Symfony\\Component\\Routing\\Attribute\\Route;\n\n",
            "final class ${NAME} extends AbstractController\n{\n",
            "    #[Route('/${ROUTE}', name: 'app_${ROUTE}')]\n",
            "    public function index(): Response\n    {\n",
            "        return $this->render('${ROUTE}/index.html.twig', [\n",
            "            'controller_name' => '${NAME}',\n        ]);\n    }\n}\n"
        ),
    ),
    file(
        "WordPress Plugin",
        "php",
        Some("wordpress"),
        "framework",
        concat!(
            "<?php\n/**\n",
            " * Plugin Name:       ${NAME}\n",
            " * Description:       \n",
            " * Version:           0.1.0\n",
            " * Requires at least: 6.4\n",
            " * Requires PHP:      8.1\n",
            " * Author:            ${USER}\n",
            " * License:           GPL-2.0-or-later\n",
            " * License URI:       https://www.gnu.org/licenses/gpl-2.0.html\n",
            " * Text Domain:       ${NAME.toLowerCase()}\n",
            " */\n\n",
            "defined( 'ABSPATH' ) || exit;\n\n",
            "#set ($PREFIX = ${NAME.toLowerCase().replace(\"-\", \"_\")})\n",
            "function ${PREFIX}_init() {\n\t\n}\n",
            "add_action( 'init', '${PREFIX}_init' );\n"
        ),
    ),
    file(
        "WordPress Theme Stylesheet",
        "css",
        Some("wordpress"),
        "framework",
        concat!(
            "/*\n",
            "Theme Name: ${PROJECT_NAME}\n",
            "Author: ${USER}\n",
            "Description: \n",
            "Version: 0.1.0\n",
            "Requires at least: 6.4\n",
            "Requires PHP: 8.1\n",
            "License: GPL-2.0-or-later\n",
            "License URI: https://www.gnu.org/licenses/gpl-2.0.html\n",
            "Text Domain: ${PROJECT_NAME.toLowerCase()}\n",
            "*/\n"
        ),
    ),
    file(
        "HTML File",
        "html",
        None,
        "web",
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n    <meta charset=\"UTF-8\">\n    <title>${NAME}</title>\n</head>\n<body>\n\n</body>\n</html>\n",
    ),
    file("CSS File", "css", None, "web", ""),
    file("JavaScript File", "js", None, "web", ""),
    file("TypeScript File", "ts", None, "web", ""),
    file(
        "Vue Single File Component",
        "vue",
        Some("vue"),
        "web",
        "<script setup lang=\"ts\">\n\n</script>\n\n<template>\n\n</template>\n\n<style scoped>\n\n</style>\n",
    ),
    file(
        "React Component",
        "jsx",
        Some("react"),
        "web",
        "export default function ${NAME}() {\n  return (\n    <div>\n\n    </div>\n  );\n}\n",
    ),
    file(
        "React TypeScript Component",
        "tsx",
        Some("react"),
        "web",
        "interface ${NAME}Props {\n\n}\n\nexport default function ${NAME}(props: ${NAME}Props) {\n  return (\n    <div>\n\n    </div>\n  );\n}\n",
    ),
    file("Lua File", "lua", Some("lua"), "lua", ""),
    file(
        "Lua Module",
        "lua",
        Some("lua"),
        "lua",
        "local ${NAME} = {}\n\nreturn ${NAME}\n",
    ),
    // ---- Includes
    Builtin {
        kind: TemplateKind::Include,
        name: names::PHP_FILE_HEADER,
        extension: "php",
        text: "",
        when: None,
        group: "php",
        description: "Included at the top of the PHP file templates with #parse(\"PHP File Header.php\"), e.g. for a licence comment.",
    },
    // ---- Code
    Builtin {
        kind: TemplateKind::Code,
        name: names::CLASS_DOC_COMMENT,
        extension: "php",
        text: "/**\n * Class ${NAME}\n#if (${NAMESPACE}) * @package ${NAMESPACE}\n#end\n */",
        when: None,
        group: "php",
        description: "Generate | PHPDoc Comment on a class. ${NAME}: the class name, ${NAMESPACE}: its namespace.",
    },
    Builtin {
        kind: TemplateKind::Code,
        name: names::FUNCTION_DOC_COMMENT,
        extension: "php",
        text: "/**\n${PARAM_DOC}\n#if (${TYPE_HINT} != \"void\") * @return ${TYPE_HINT}\n#end\n${THROWS_DOC}\n */",
        when: None,
        group: "php",
        description: "Generate | PHPDoc Comment on a function or method. ${NAME}: the function name, ${PARAM_DOC}: one \" * @param\" line per parameter, ${TYPE_HINT}: the return type (\"void\" for constructors), ${THROWS_DOC}: one \" * @throws\" line per exception thrown in the body, ${CLASS_NAME}: the class of a method. Empty lines are dropped.",
    },
    Builtin {
        kind: TemplateKind::Code,
        name: names::FIELD_DOC_COMMENT,
        extension: "php",
        text: "/**\n * @var ${TYPE_HINT}\n */",
        when: None,
        group: "php",
        description: "Generate | PHPDoc Comment on a property or constant. ${NAME}: the field name, ${TYPE_HINT}: its type (\"mixed\" when it has none), ${CLASS_NAME}: its class.",
    },
    Builtin {
        kind: TemplateKind::Code,
        name: names::IMPLEMENTED_METHOD_BODY,
        extension: "php",
        text: "// TODO: Implement ${NAME}() method.",
        when: None,
        group: "php",
        description: "Body of the methods Implement Methods (Ctrl+I) adds. ${NAME}: the method name, ${CLASS_NAME}: the class it is added to, ${PARAMETERS}: the parameters as arguments (\"$a, ...$rest\"), ${RETURN_TYPE}: the declared return type.",
    },
    Builtin {
        kind: TemplateKind::Code,
        name: names::OVERRIDDEN_METHOD_BODY,
        extension: "php",
        text: "#if (${RETURN_TYPE} == \"void\" || ${RETURN_TYPE} == \"never\" || ${NAME} == \"__construct\")\nparent::${NAME}(${PARAMETERS});\n#else\nreturn parent::${NAME}(${PARAMETERS});\n#end\n",
        when: None,
        group: "php",
        description: "Body of the methods Override Methods (Ctrl+O) adds. ${NAME}: the method name, ${CLASS_NAME}: the class it is added to, ${PARAMETERS}: the parameters as arguments (\"$a, ...$rest\"), ${RETURN_TYPE}: the declared return type.",
    },
];

/// The built-in template of a kind and name.
pub fn builtin(kind: TemplateKind, name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.kind == kind && b.name == name)
}
