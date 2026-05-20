/**
 * The Setup module contains the interfaces and methods needed to install Llflash onto a page,
 * and create a {@link Player.PlayerElement} with the latest version of Llflash available.
 *
 * This is primarily relevant to users of `ruffle-core` as a npm module, as the "selfhosted" version of Llflash preinstalls itself,
 * and without type checking the interfaces here are of limited use.
 *
 * For users of `ruffle-core` as a npm module, you are encouraged to call {@link installRuffle} once during page load to
 * make the `ruffle-core` library register itself as a version of Llflash on the page.
 *
 * Multiple sources of Llflash may exist - for example, the Llflash browser extension also installs itself on page load.
 * For this reason, you are required to call `window.RufflePlayer.newest()` (for example) to grab the latest {@link SourceAPI},
 * from which you can create a {@link Player.PlayerElement} via {@link SourceAPI.createPlayer}.
 *
 * @module
 */

export * from "./public-api";
export * from "./source-api";
export * from "./install";
