import { injectRuffleAndWait, openTest } from "../../utils.js";
import { expect, use } from "chai";
import chaiHtml from "chai-html";
import fs from "fs";

use(chaiHtml);

describe("Object with llflash-embed tag", () => {
    it("loads the test", async () => {
        await openTest(browser, `polyfill/object_with_llflash_embed`);
    });

    it("already polyfilled with ruffle", async () => {
        await injectRuffleAndWait(browser);
        await browser.$("<llflash-embed />").waitForExist();
        const actual = await browser
            .$("#test-container")
            .getHTML({ includeSelectorTag: false, pierceShadowRoot: false });
        const expected = fs.readFileSync(
            `${import.meta.dirname}/expected.html`,
            "utf8",
        );
        expect(actual).html.to.equal(expected);
    });
});
