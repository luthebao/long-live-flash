# llflash-core

llflash-core is the core javascript bindings to the Wasm ruffle-web binary,
and contains the actual public API.

## Using llflash-core

For more examples and in-depth documentation on how to use Llflash on your website, please
check out our wiki.

### Host Llflash

The `core` package is configured for websites that build and bundle their files themselves.
Simply add `ruffle` to an npm package, and use something like Webpack to actually bundle and serve
the files.

If you wish to use Llflash on a website that doesn't use npm, we have a pre-bundled version which
we call 'selfhosted'. Please refer to its documentation.

Please note that the `.wasm` file must be served properly, and some web servers may not do that
correctly out of the box. Please see our wiki
for instructions on how to configure this, if you encounter a `Incorrect response MIME type` error.

### "Plug and Play"

If you have an existing website with flash content, you can simply include Llflash as a script and
our polyfill magic will replace everything for you. No fuss, no mess.

```html
<script src="path/to/ruffle/llflash.js"></script>
```

### Javascript API

If you want to control the Llflash player, you may use our Javascript API.

```html
<script>
    window.RufflePlayer = window.RufflePlayer || {};

    window.addEventListener("DOMContentLoaded", () => {
        let ruffle = window.RufflePlayer.newest();
        let player = ruffle.createPlayer();
        let container = document.getElementById("container");
        container.appendChild(player);
        player.ruffle().load("movie.swf");
    });
</script>
<script src="path/to/ruffle/llflash.js"></script>
```

## Building, testing or contributing

Please see [the ruffle-web README](../../README.md).
