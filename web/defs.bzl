"""The web version: static files plus pdftools-cli built for WebAssembly."""

def _wasm_transition_impl(_settings, _attr):
    return {"//command_line_option:platforms": "//:wasm32_wasip1"}

_wasm_transition = transition(
    implementation = _wasm_transition_impl,
    inputs = [],
    outputs = ["//command_line_option:platforms"],
)

def _web_site_impl(ctx):
    outputs = []

    def add(src, path):
        out = ctx.actions.declare_file(ctx.label.name + "/" + path)
        ctx.actions.symlink(output = out, target_file = src)
        outputs.append(out)

    for src in ctx.files.srcs:
        add(src, src.basename)
    for src in ctx.files.icons:
        add(src, "icons/" + src.basename)
    add(ctx.file.favicon, "icon.png")
    add(ctx.files.wasm[0], "pdftools-cli.wasm")
    return [DefaultInfo(files = depset(outputs), runfiles = ctx.runfiles(files = outputs))]

web_site = rule(
    implementation = _web_site_impl,
    doc = "Collects the page, its icons and the WebAssembly module into one folder.",
    attrs = {
        "srcs": attr.label_list(allow_files = True),
        "icons": attr.label_list(allow_files = [".svg"]),
        "favicon": attr.label(allow_single_file = True),
        "wasm": attr.label(cfg = _wasm_transition),
    },
)

def _web_server_impl(ctx):
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    folder = ctx.attr.site.label.package + "/" + ctx.attr.site.label.name
    ctx.actions.write(
        output = script,
        is_executable = True,
        content = """#!/bin/sh
# Serves the web version on http://localhost:${{PORT:-8080}}.
cd "${{RUNFILES_DIR:-$0.runfiles}}/_main/{folder}" 2>/dev/null || cd "{folder}"
echo "PDF Tools: http://localhost:${{PORT:-8080}}"
exec python3 -m http.server "${{PORT:-8080}}" --bind 127.0.0.1
""".format(folder = folder),
    )
    return [DefaultInfo(executable = script, runfiles = ctx.attr.site[DefaultInfo].default_runfiles)]

web_server = rule(
    implementation = _web_server_impl,
    doc = "`bazel run` serves the site locally (needs python3).",
    attrs = {"site": attr.label()},
    executable = True,
)
