/**
 * Language packs for scripts and configuration formats: Bash, Python, YAML, JSON, INI, .conf, .env,
 * ignore files, Dockerfile, Docker Compose, Postfix, E-Mail and Symfony services. Each file
 * registers its language (and editor extensions) on import; the file types are declared in
 * the plugin manifests `app/src-tauri/plugins/langs.toml`.
 */
import "./bash";
import "./python";
import "./yaml";
import "./json";
import "./env";
import "./ini";
import "./ignore";
import "./dockerfile";
import "./compose";
import "./postfix";
import "./eml";
import "./symfony";
