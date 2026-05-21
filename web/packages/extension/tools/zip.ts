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
    await fs.rm(destination, { force: true });
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

// `--unpacked=<dir>`: also write the same effective contents to <dir> so
// Chrome's "Load unpacked" picks up the latest build on reload. The dir
// is wiped first so stale files from a prior version don't survive.
async function unpack(source: string, destination: string, stripKey: boolean) {
    await fs.rm(destination, { recursive: true, force: true });
    await fs.cp(source, destination, { recursive: true });
    if (stripKey) {
        const manifestPath = path.join(destination, "manifest.json");
        const manifest = JSON.parse(await fs.readFile(manifestPath, "utf8"));
        delete manifest.key;
        await fs.writeFile(manifestPath, JSON.stringify(manifest));
    }
}

const assets = url.fileURLToPath(new URL("../assets/", import.meta.url));
const positional = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const stripKey = process.argv.includes("--strip-key");
const unpackedFlag = process.argv.find((a) => a.startsWith("--unpacked="));
const unpackedDir = unpackedFlag?.split("=", 2)[1];

async function run() {
    await zip(assets, positional[0] ?? "", stripKey);
    if (unpackedDir) {
        await unpack(assets, unpackedDir, stripKey);
    }
}

run().catch(console.error);
