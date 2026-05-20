message-cant-embed =
    Llflash nije mogao pokrenuti Flash ugrađen na ovoj stranici.
    Možete pokušati otvoriti datoteku u zasebnoj kartici kako biste izbjegli ovaj problem.
message-restored-from-bfcache =
    Vaš preglednik je vratio ovaj Flash sadržaj iz prethodne sesije.
    Molimo vas da ponovo učitate stranicu za novi početak.
panic-title = Nešto je pošlo po zlu :(
more-info = Dodatne informacije
run-anyway = Ipak pokreni
continue = Nastavi
report-bug = Prijavi grešku
update-ruffle = Ažuriraj Llflash
ruffle-demo = Web probna verzija
ruffle-desktop = Desktop aplikacija
ruffle-wiki = Pogledaj Llflash Wiki
enable-hardware-acceleration = Izgleda da je hardversko ubrzanje onemogućeno. Iako Llflash možda radi, moguće je da je vrlo spor. Možete saznati kako omogućiti hardversko ubrzanje slijedeći link ispod:
enable-hardware-acceleration-link = Često postavljana pitanja - Hardversko ubrzanje u Chromeu
view-error-details = Prikaži detalje greške
open-in-new-tab = Otvori u novoj kartici
click-to-unmute = Kliknite da biste uključili zvuk
clipboard-message-title = Kopiranje i naljepljivanje u Llflash-u
clipboard-message-description =
    { $variant ->
    *[unsupported] Vaš preglednik ne podržava potpuni pristup međuspremniku,
    [access-denied] Pristup međuspremniku je odbijen,
    } ali uvijek možete koristiti ove prečice:
clipboard-message-copy = { " " } za kopiranje
clipboard-message-cut = { " " } za isijecanje
clipboard-message-paste = { " " } za lijepljenje
error-canvas-reload = Nije moguće ponovo učitati renderer kada je renderer već u upotrebi.
error-file-protocol =
    Izgleda da koristite Llflash na protokolu "file:".
    Ovo ne funkcioniše jer preglednici blokiraju mnoge funkcije iz sigurnosnih razloga.
    Umjesto toga, preporučujemo vam da postavite lokalni server ili koristite web probnu verziju ili aplikaciju.
error-javascript-config =
    Llflash je naišao na ozbiljan problem zbog pogrešne konfiguracije JavaScript-a.
    Ako ste administrator servera, preporučujemo vam da provjerite detalje greške kako biste saznali koji parametar uzrokuje problem. Također možete konsultovati Llflash wiki za pomoć.
error-wasm-not-found =
    Llflash nije uspio učitati potrebnu komponentu datoteke ".wasm".
    Ako ste administrator servera, provjerite je li datoteka ispravno otpremljena.
    Ako problem i dalje postoji, možda ćete morati koristiti postavku "publicPath": obratite se Llflash wiki stranici za pomoć.
error-wasm-mime-type =
    Llflash je naišao na ozbiljan problem prilikom pokušaja inicijalizacije.
    Ovaj web server ne poslužuje ".wasm" datoteke s ispravnim MIME tipom.
    Ako ste administrator servera, molimo vas da se obratite Llflash wiki stranici za pomoć.
error-invalid-swf =
    Llflash ne može analizirati traženu datoteku.
    Najvjerovatniji razlog je taj što tražena datoteka nije važeći SWF.
error-swf-fetch =
    Llflash nije uspio učitati Flash SWF datoteku.
    Najvjerovatniji razlog je taj što datoteka više ne postoji, tako da Llflash nema šta učitati.
    Pokušajte kontaktirati administratora web stranice za pomoć.
error-swf-cors =
    Llflash nije uspio učitati Flash SWF datoteku.
    Pristup za preuzimanje je vjerovatno blokiran CORS politikom.
    Ako ste administrator servera, obratite se Llflash wiki stranici za pomoć.
error-wasm-cors =
    Llflash nije uspio učitati potrebnu komponentu datoteke ".wasm".
    Pristup dohvatu je vjerovatno blokiran CORS politikom.
    Ako ste administrator servera, obratite se Llflash wiki stranici za pomoć.
error-wasm-invalid =
    Llflash je naišao na ozbiljan problem prilikom pokušaja inicijalizacije.
    Izgleda da ovoj stranici nedostaju ili su datoteke nevažeće za pokretanje Rufflea.
    Ako ste administrator servera, pogledajte Llflash wiki za pomoć.
error-wasm-download =
    Llflash je naišao na ozbiljan problem prilikom pokušaja inicijalizacije.
    Ovo se često može riješiti jednostavnim ponovnim učitavanjem stranice.
    U suprotnom, kontaktirajte administratora stranice.
error-wasm-disabled-on-edge =
    Llflash nije uspio učitati potrebnu komponentu datoteke ".wasm".
    Da biste riješili ovaj problem, pokušajte otvoriti postavke preglednika, kliknuti na "Privatnost, pretraga i usluge", pomaknuti se prema dolje i isključiti "Poboljšanje web sigurnosti".
    Ovo će omogućiti vašem pregledniku da učita potrebne datoteke ".wasm".
    Ako problem i dalje postoji, možda ćete morati koristiti drugi preglednik.
error-wasm-unsupported-browser =
    Preglednik koji koristite ne podržava WebAssembly ekstenzije potrebne za rad Llflash-a.
    Molimo vas da pređete na podržani preglednik.
    Popis podržanih preglednika možete pronaći na Wiki stranici.
error-javascript-conflict =
    Llflash je naišao na ozbiljan problem prilikom pokušaja inicijalizacije.
    Izgleda da ova stranica koristi JavaScript kod koji je u sukobu sa Ruffleom.
    Ako ste administrator servera, pozivamo vas da pokušate otpremiti datoteku na praznu stranicu.
error-javascript-conflict-outdated = Također možete pokušati prenijeti noviju verziju Rufflea koja bi mogla riješiti problem (trenutna verzija je zastarjela: { $buildDate }).
error-csp-conflict =
    Llflash je naišao na ozbiljan problem prilikom pokušaja inicijalizacije.
    Politike sigurnosti sadržaja ovog web servera ne dozvoljavaju pokretanje potrebne komponente ".wasm".
    Ako ste administrator servera, obratite se Llflash wiki stranici za pomoć.
error-unknown =
    Llflash je naišao na ozbiljan problem prilikom pokušaja prikazivanja ovog Flash sadržaja.
    { $outdated ->
    [true] Ako ste administrator servera, pokušajte prenijeti noviju verziju Rufflea (trenutna verzija je zastarjela: { $buildDate }).
    *[false] Ovo se ne bi trebalo dogoditi, pa bismo vam bili jako zahvalni ako biste prijavili grešku!
    }
