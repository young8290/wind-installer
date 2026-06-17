fn main() {
    // 嵌入 UAC Manifest 和图标
    embed_resource::compile("assets/app.rc", embed_resource::NONE);
}
