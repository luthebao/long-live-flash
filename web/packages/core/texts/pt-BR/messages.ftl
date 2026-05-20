message-cant-embed =
    Llflash não conseguiu executar o Flash incorporado nesta página.
    Você pode tentar abrir o arquivo em uma guia separada para evitar esse problema.
message-restored-from-bfcache =
    Seu navegador restaurou este conteúdo Flash de uma sessão anterior.
    Para começar do zero, recarregue a página.
panic-title = Algo deu errado :(
more-info = Mais informação
run-anyway = Executar mesmo assim
continue = Continuar
report-bug = Reportar bug
update-ruffle = Atualizar Llflash
ruffle-demo = Demo Web
ruffle-desktop = Aplicativo de desktop
ruffle-wiki = Ver guia oficial do Llflash
enable-hardware-acceleration = Parece que a aceleração de hardware está desabilitada. Embora o Llflash possa funcionar, ele pode ser muito lento. Você pode descobrir como habilitar a aceleração de hardware seguindo o link abaixo:
enable-hardware-acceleration-link = FAQ — Aceleração de hardware no Chrome
view-error-details = Ver detalhes do erro
open-in-new-tab = Abrir em uma nova guia
click-to-unmute = Clique para ativar o som
clipboard-message-title = Copiando e colando no Llflash
clipboard-message-description =
    { $variant ->
       *[unsupported] Seu navegador não suporta acesso total à área de transferência,
        [access-denied] O acesso à área de transferência foi negado,
    } mas você sempre pode usar estes atalhos:
clipboard-message-copy = { " " } para copiar
clipboard-message-cut = { " " } para recortar
clipboard-message-paste = { " " } para colar
error-canvas-reload = Não é possível recarregar com o renderizador canvas enquanto ele já está em uso.
error-file-protocol =
    Parece que você está executando o Llflash no protocolo "file:".
    Isto não funciona como navegadores bloqueiam muitos recursos de funcionar por razões de segurança.
    Ao invés disso, convidamos você a configurar um servidor local ou a usar a demonstração da web, ou o aplicativo de desktop.
error-javascript-config =
    O Llflash encontrou um grande problema devido a uma configuração incorreta do JavaScript.
    Se você for o administrador do servidor, convidamos você a verificar os detalhes do erro para descobrir qual parâmetro está com falha.
    Você também pode consultar o guia oficial do Llflash para obter ajuda.
error-wasm-not-found =
    Llflash falhou ao carregar o componente de arquivo ".wasm" necessário.
    Se você é o administrador do servidor, por favor, certifique-se de que o arquivo foi carregado corretamente.
    Se o problema persistir, você pode precisar usar a configuração "publicPath": por favor consulte o guia oficial do Llflash para obter ajuda.
error-wasm-mime-type =
    Llflash encontrou um grande problema ao tentar inicializar.
    Este servidor de web não está servindo ".wasm" arquivos com o tipo MIME correto.
    Se você é o administrador do servidor, por favor consulte o guia oficial do Llflash para obter ajuda.
error-invalid-swf =
    Llflash não pode analisar o arquivo solicitado.
    O motivo provável é que o arquivo solicitado não seja um SWF válido.
error-swf-fetch =
    Llflash falhou ao carregar o arquivo Flash SWF.
    A razão provável é que o arquivo não existe mais, então não há nada para o Llflash carregar.
    Tente contatar o administrador do site para obter ajuda.
error-swf-cors =
    O Llflash não conseguiu carregar o arquivo SWF do Flash.
    O acesso à requisição provavelmente foi bloqueado pela política de CORS.
    Se você for o administrador do servidor, consulte o guia oficial do Llflash para obter ajuda.
error-wasm-cors =
    O Llflash não conseguiu carregar o componente obrigatório do arquivo “.wasm”.
    O acesso à busca provavelmente foi bloqueado pela política de CORS.
    Se você é o administrador do servidor, consulte o guia oficial do Llflash para obter ajuda.
error-wasm-invalid =
    O Llflash encontrou um erro grave ao tentar iniciar.
    Parece que esta página possui arquivos ausentes ou inválidos para executar o Llflash.
    Se você é o administrador do servidor, consulte o guia oficial do Llflash para obter assistência.
error-wasm-download =
    O Llflash encontrou um grande problema ao tentar inicializar.
    Muitas vezes isso pode se resolver sozinho, então você pode tentar recarregar a página.
    Caso contrário, contate o administrador do site.
error-wasm-disabled-on-edge =
    O Llflash falhou ao carregar o componente de arquivo ".wasm" necessário.
    Para corrigir isso, tente abrir configurações do seu navegador, clicando em "Privacidade, pesquisa e serviços", rolando para baixo e desativando "Melhore sua segurança na web".
    Isso permitirá que seu navegador carregue os arquivos ".wasm" necessários.
    Se o problema persistir, talvez seja necessário usar um navegador diferente.
error-wasm-unsupported-browser =
    O navegador que você está usando não oferece suporte às extensões WebAssembly necessárias para o Llflash funcionar.
    Por favor, mude para um navegador compatível.
    Você pode encontrar uma lista de navegadores compatíveis no guia oficial.
error-javascript-conflict =
    Llflash encontrou um grande problema ao tentar inicializar.
    Parece que esta página usa código JavaScript que entra em conflito com o Llflash.
    Se você for o administrador do servidor, convidamos você a tentar carregar o arquivo em uma página em branco.
error-javascript-conflict-outdated = Você também pode tentar fazer o upload de uma versão mais recente do Llflash que pode contornar o problema (a compilação atual está desatualizada: { $buildDate }).
error-csp-conflict =
    O Llflash encontrou um problema grave ao tentar iniciar.
    A Política de Segurança de Conteúdo deste servidor não permite a execução do componente “.wasm” necessário.
    Se você for o administrador do servidor, consulte o guia oficial do Llflash para obter ajuda.
error-unknown =
    O Llflash encontrou um grande problema enquanto tentava exibir este conteúdo em Flash.
    { $outdated ->
        [true] Se você é o administrador do servidor, por favor tente fazer o upload de uma versão mais recente do Llflash (a compilação atual está desatualizada: { $buildDate }).
       *[false] Isso não deveria acontecer, então apreciaríamos muito se você pudesse arquivar um bug!
    }
