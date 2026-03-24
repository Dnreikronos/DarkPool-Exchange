# Architecture Decisions — ZK Dark Pool DEX

> Decisions made: 2026-03-23

---

## 1. Trust Model do Matching Engine

**Problema:** Price-time priority exige conhecer o preco, mas o design original diz que o engine so ve commitments. Isso e uma contradicao.

### Opcoes consideradas

**Opcao A — Operador semi-trusted (pragmatica)** ✅ ESCOLHIDA

O trader encripta a ordem para o engine usando a chave publica do operador. O engine decripta, faz matching normal em cleartext, e gera uma prova ZK de que executou o matching corretamente (sem favorecer ninguem, sem front-running). A privacidade e contra o mundo externo, nao contra o operador.

- Pros: simples de implementar, matching engine continua sendo um btree normal em Go, performance maxima
- Contras: o operador ve as ordens — se for malicioso pode vazar info (mas a prova ZK garante que ele nao adulterou o matching)
- Referencia: a maioria das dark pools institucionais em TradFi funciona assim

**Opcao B — MPC entre multiplos operadores**

Dividir o engine em 2-3 nos que fazem matching via MPC (secure multi-party computation). Nenhum no individual ve a ordem completa. O matching acontece sobre shares criptograficos.

- Pros: nenhum operador individual ve as ordens, trust model muito forte
- Contras: latencia alta (MPC adiciona rounds de comunicacao), throughput cai drasticamente (esquecer 100k/s, talvez 1k/s), complexidade de implementacao e enorme
- Referencia: Renegade Protocol usa esse approach

**Opcao C — Crossing network com preco externo**

Eliminar price-time priority. Usar um preco de referencia externo (oracle tipo Chainlink/Pyth). Ordens sao so "compra X unidades" ou "vende Y unidades" com um limit price escondido no commitment. O engine faz crossing periodico: no momento do crossing, revela quais ordens cruzam no preco do oracle e executa todas ao mesmo preco.

- Pros: engine nao precisa saber preco individual de cada ordem — so precisa verificar se commitment satisfaz a condicao "limit price >= oracle price" (pode ser feito em ZK). Privacidade real contra todos
- Contras: sem price discovery propria, dependencia de oracle, execucao nao e continua (batched em intervalos), UX diferente do que traders esperam
- Referencia: Penumbra tem elementos desse modelo

### Decisao

Opcao A. E honesta, funcional, e permite demonstrar o sistema end-to-end. O trust model sera documentado explicitamente no whitepaper. A porta fica aberta para migrar para Opcao C no futuro.

---

## 2. Price Discovery

**Problema:** Se ninguem ve as ordens, como o mercado forma preco?

### Opcoes consideradas

**Opcao A — Oracle externo como referencia + order book privado**

Usar Chainlink/Pyth como preco de referencia publico. O order book interno do engine tem os precos reais (no modelo semi-trusted), mas o frontend mostra apenas profundidade agregada e anonimizada — tipo "ha X volume entre $1800-$1850" sem revelar ordens individuais.

- Pros: o mercado externo faz o price discovery, o dark pool funciona como venue de execucao com preco justo verificavel
- Contras: nao contribui pra price discovery, e parasitario do preco de outras venues (mas dark pools em TradFi tambem sao)

**Opcao B — Leilao periodico com revelacao atrasada** ✅ ESCOLHIDA

A cada N segundos (ex: 5s), todas as ordens do periodo entram num batch auction. Preco de clearing e calculado e revelado apos execucao. O historico de precos de clearing serve como price discovery.

- Pros: preco emerge do proprio mercado, sem dependencia de oracle, mais justo (ninguem tem vantagem temporal dentro do batch)
- Contras: latencia minima de N segundos, traders de HFT nao vao gostar, logica de batch auction e mais complexa que matching continuo

**Opcao C — Modelo hibrido**

Duas camadas: ordens "publicas" no order book anonimizado (mostram faixa de preco, nao preco exato) fazem o price discovery. Ordens "escuras" sao totalmente privadas e executam contra o preco formado pelas publicas.

- Pros: price discovery natural + privacidade maxima pra quem quer
- Contras: dois tipos de ordem = mais complexidade no engine, precisa de massa critica de ordens publicas pra funcionar

### Decisao

Opcao B. O preco emerge do proprio mercado via batch auction periodico. Sem dependencia de oracles externos, e o modelo e mais justo — ninguem tem vantagem temporal dentro do batch. A latencia de N segundos e aceitavel para o caso de uso (privacidade > velocidade).

---

## 3. Consistencia do WAL (crash entre match e batch submit)

**Problema:** Se o engine crasha entre match e submit do batch on-chain, pode perder estado. WAL sozinho nao garante consistencia completa.

### Opcoes consideradas

**Opcao A — WAL + snapshots periodicos**

O WAL grava toda operacao (insert, cancel, match). A cada N operacoes ou M segundos, tira um snapshot do estado completo do order book. Na recuperacao: carrega ultimo snapshot + replays do WAL a partir daquele ponto.

- Pros: recovery rapido (nao precisa replayar desde o inicio), implementacao straightforward
- Contras: snapshots consomem disco e tem um custo de I/O momentaneo

**Opcao B — Event sourcing completo** ✅ ESCOLHIDA

Nao manter estado mutavel. O order book e uma projecao derivada de uma sequencia de eventos imutaveis (OrderPlaced, OrderMatched, OrderCancelled, BatchSubmitted). O engine reconstroi o estado replayando eventos. O batch so e considerado "committed" quando o evento BatchSubmitted e persistido.

- Pros: auditabilidade total, facil de debugar ("o que aconteceu na sequencia X?"), replay deterministico, natural pra um sistema financeiro
- Contras: recovery pode ser lento se o log ficar grande (resolve com compaction), mais abstrato de implementar
- Referencia: LMAX Exchange popularizou esse padrao para matching engines

**Opcao C — WAL com two-phase commit pro batch**

O WAL marca o batch como "pending" antes de submeter on-chain. Depois do tx confirmar, marca como "committed". No recovery, batches pending sao re-submetidos ou revertidos.

- Pros: resolve especificamente o gap entre match e settlement, nao muda o resto da arquitetura
- Contras: precisa lidar com idempotencia (o batch pode ja ter sido submetido on-chain antes do crash)

### Decisao

Opcao B. Event sourcing e o padrao da industria para matching engines. Mais trabalho no inicio, mas simplifica recovery, auditoria, debugging e replay de cenarios.

---

## 4. Aggregator Go + Rust

**Problema:** O aggregator mistura Go e Rust sem beneficio claro. Vale simplificar.

### Opcoes consideradas

**Opcao A — Rust puro via CLI** ✅ ESCOLHIDA

O aggregator e um binario Rust. O Go chama via `exec.Command()`, passa os proofs como arquivo ou stdin, recebe a prova agregada como output.

- Pros: zero complexidade de FFI, facil de testar o aggregator isoladamente, deploy simples
- Contras: overhead de spawn de processo (mas e por batch, nao por ordem — 1 call a cada 256 matches e irrelevante)

**Opcao B — Rust via FFI (cgo)**

Compilar o aggregator como `.so`/`.dll` e chamar via cgo.

- Pros: sem overhead de processo, chamada direta
- Contras: cgo desabilita algumas otimizacoes do Go scheduler, cross-compilation vira um pesadelo, debugging fica mais dificil

**Opcao C — Aggregator como microservico Rust separado**

Um servico Rust standalone com uma API gRPC. O Go manda os proofs via gRPC, recebe a prova agregada.

- Pros: desacoplamento total, pode escalar independentemente, cada servico no seu runtime ideal
- Contras: mais infra pra manter (mais um servico, mais um deploy), latencia de rede (minima em localhost)

### Decisao

Opcao A. CLI e o mais simples e o overhead e negligivel — estamos falando de 1 chamada por batch de 256 trades, e a prova agregada ja demora segundos pra computar. Spawn de processo e ruido. Se escalar virar necessidade, migra pra Opcao C.

---

## Resumo

| Problema | Decisao | Motivo |
|---|---|---|
| Trust model | Semi-trusted (Opcao A) | Funcional, honesto, simples |
| Price discovery | Batch auction periodico (Opcao B) | Preco emerge do mercado, sem oracle |
| Consistencia | Event sourcing (Opcao B) | Padrao da industria pra engines |
| Aggregator | CLI Rust (Opcao A) | Overhead irrelevante, maxima simplicidade |
