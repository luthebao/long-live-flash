import fs from "fs/promises";
import path from "path";
import url from "url";
import archiver from "archiver";

// `--strip-key`: drop the top-level `key` from manifest.json on the way
// into the zip. Required for Chrome Web Store uploads — CWS assigns the
// extension ID at publish time and rejects any manifest that pre-sets it.
// The key stays on disk in assets/manifest.json so "Load unpacked" still
// produces the stable dev ID (mcahcaahgbcmapfdcekcjpdopagoncec).
async function zip(source: string, destination: string, stripKey: boolean) {
    await fs.mkdir(path.dirname(destination), { recursive: true });
    const output = (await fs.open(destination, "w")).createWriteStream();
    const archive = archiver("zip");

    output.on("close", () => {
        console.log(
            `Extension is ${archive.pointer()} total bytes when packaged.`,
        );
    });

    archive.on("error", (error) => {
        throw error;
    });

    archive.on("warning", (error) => {
        if (error.code === "ENOENT") {
            console.warn(`Warning whilst zipping extension: ${error}`);
        } else {
            throw error;
        }
    });

    archive.pipe(output);

    if (stripKey) {
        const manifestPath = path.join(source, "manifest.json");
        const manifest = JSON.parse(await fs.readFile(manifestPath, "utf8"));
        delete manifest.key;
        archive.append(JSON.stringify(manifest), { name: "manifest.json" });
        archive.glob("**/*", { cwd: source, ignore: ["manifest.json"] });
    } else {
        archive.directory(source, "");
    }

    await archive.finalize();
}
const assets = url.fileURLToPath(new URL("../assets/", import.meta.url));
const positional = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const stripKey = process.argv.includes("--strip-key");
zip(assets, positional[0] ?? "", stripKey).catch(console.error);
