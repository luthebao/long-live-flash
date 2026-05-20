# llflash-selfhosted

llflash-selfhosted is the intended way to get Llflash onto your website.

You may either include it and forget about it, and we will polyfill existing Flash content,
or use our APIs for custom configurations or more advanced usages of the Llflash player.

## Using llflash-selfhosted

For more examples and in-depth documentation on how to use Llflash on your website, please
check out our wiki.

### Host Llflash

The `selfhosted` package is configured for websites that do not use bundlers or npm and just want
to get up and running. If you'd prefer to use Llflash through npm and a bundler, please
refer to ruffle core.

Before you can get started with using Llflash on your website, you must host its files yourself.
Either take the latest build
or build it yourself, and make these files accessible by your web server.

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

Please see the ruffle-web README.
