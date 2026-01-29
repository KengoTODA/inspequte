# Changelog

## [0.9.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.8.1...inspequte-v0.9.0) (2026-01-29)


### Features

* add interrupted exception handling rule ([3e1841d](https://github.com/KengoTODA/inspequte/commit/3e1841da698bb51ba167d2663852230e3f20e283))
* add prefer enumset rule ([23f03d1](https://github.com/KengoTODA/inspequte/commit/23f03d1a8b368ab55ff0bc6461ac892fd96ddc9c))

## [0.8.1](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.8.0...inspequte-v0.8.1) (2026-01-26)


### Bug Fixes

* **deps:** fix parsing process to enable analysis on SonarQube ([92f5815](https://github.com/KengoTODA/inspequte/commit/92f58155cdc21184ac0591076f12210511bdc9dd))

## [0.8.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.7.0...inspequte-v0.8.0) (2026-01-25)


### Features

* support [@file](https://github.com/file) inputs for huge classfile input ([e9e43d6](https://github.com/KengoTODA/inspequte/commit/e9e43d6604bc88101483e89f2ea070d4a3f1ee8d))

## [0.7.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.6.1...inspequte-v0.7.0) (2026-01-22)


### Features

* focus analysis targets with classpath ([aa6f4f4](https://github.com/KengoTODA/inspequte/commit/aa6f4f4b5da8de16f62401d940846fc0585bfb47))
* **telemetry:** add hierarchical span names ([8d67b9d](https://github.com/KengoTODA/inspequte/commit/8d67b9d1ada0b08a4c844a45fe3e71f24ede8e1f))
* **telemetry:** add sarif.build span ([21f59e9](https://github.com/KengoTODA/inspequte/commit/21f59e9525d8238e6ddf9da7ff340fd8d93f49d7))


### Bug Fixes

* **engine:** handle nested jar uris ([be3b3b1](https://github.com/KengoTODA/inspequte/commit/be3b3b16578e44d5fe5277d8a8a1cff40164dd17))
* prevent array-equals false positives ([de7530c](https://github.com/KengoTODA/inspequte/commit/de7530c2265b6e84b1fa008ab4eb6eb82a9d51e0))
* refine null checks in nullness flow ([b3849f8](https://github.com/KengoTODA/inspequte/commit/b3849f8bc297e2a87d63a0c4414b248c6284f44b))
* remove dead code rule ([bf7d7a2](https://github.com/KengoTODA/inspequte/commit/bf7d7a2e271c223d5c31979e2ec1c9805a3537b2))
* skip empty catch in non-target classes ([fce93f9](https://github.com/KengoTODA/inspequte/commit/fce93f92ca837e493821003b92cdeaf4de204224))
* skip implicit initializers in dead code ([57f8cc5](https://github.com/KengoTODA/inspequte/commit/57f8cc586dc4db2a1bd4f20f8992e8241d87e32b))
* skip lambda methods in dead code ([acd2955](https://github.com/KengoTODA/inspequte/commit/acd29558b493f0e9bcbd59476ab65e6ac953b77d))
* **telemetry:** improve sarif related spans ([eb8bbf3](https://github.com/KengoTODA/inspequte/commit/eb8bbf3bd5acc80a571b6511429be29dc24193e7))
* **telemetry:** stabilize otlp export ([cb0556b](https://github.com/KengoTODA/inspequte/commit/cb0556b7bbb95904e46838a47982151ba0bbe757))


### Performance Improvements

* **output:** speed up sarif serialization ([1ddfc84](https://github.com/KengoTODA/inspequte/commit/1ddfc8474eef648f94fd768931ff14010ae53607))
* **scan:** parallelize jar.scan ([5fd39d1](https://github.com/KengoTODA/inspequte/commit/5fd39d163096b6e0eb624f6632b6911862377f80))

## [0.6.1](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.6.0...inspequte-v0.6.1) (2026-01-22)


### Bug Fixes

* parent class spans under jar scan ([13d551e](https://github.com/KengoTODA/inspequte/commit/13d551e79b705c873f433d949c5c178ad461abfc))
* restore jar scan result handling ([4bb4267](https://github.com/KengoTODA/inspequte/commit/4bb426726c0206e449baaec3a0c4b32e56401507))

## [0.6.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.5.1...inspequte-v0.6.0) (2026-01-22)


### Features

* add slf4j format const rule ([57898d9](https://github.com/KengoTODA/inspequte/commit/57898d9097c521100bcc924e22d5bdad4ddbd956))
* add slf4j illegal passed class rule ([306cd89](https://github.com/KengoTODA/inspequte/commit/306cd89adc6b2edd2fc9f6f5542e9d5a76beb086))
* add slf4j logger final rule ([b73fe2b](https://github.com/KengoTODA/inspequte/commit/b73fe2b75b494915fdd0541a2bc41909748d5dba))
* add slf4j preformatted message rule ([a99e9ad](https://github.com/KengoTODA/inspequte/commit/a99e9adf78b928c4bf0192b84800e6800d4a74fb))
* add slf4j private logger rule ([476929d](https://github.com/KengoTODA/inspequte/commit/476929d864344adb74e84dde31980197d62b3ba7))
* add slf4j sign-only format rule ([cae64d8](https://github.com/KengoTODA/inspequte/commit/cae64d8a2e9fc77bbf68c54c2113554c53f00560))
* add slf4j unknown array rule ([543c8c6](https://github.com/KengoTODA/inspequte/commit/543c8c6cbfc6f9d1e1cb00f296c1600a97f17e93))

## [0.5.1](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.5.0...inspequte-v0.5.1) (2026-01-22)


### Performance Improvements

* cache call graph resolutions ([6628b60](https://github.com/KengoTODA/inspequte/commit/6628b603610128f38b9e4657e85cd548a412088f))
* run rules in parallel ([2546771](https://github.com/KengoTODA/inspequte/commit/254677140b5bc2aa875118e26eed357605b9a6a0))

## [0.5.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.4.0...inspequte-v0.5.0) (2026-01-22)


### Features

* add otel tracing export ([#16](https://github.com/KengoTODA/inspequte/issues/16)) ([45f60ec](https://github.com/KengoTODA/inspequte/commit/45f60ec3bacbe5d8a93412a0fe8dba57455e830e))
* **rules:** add SLF4J placeholder mismatch rule ([fba3cc3](https://github.com/KengoTODA/inspequte/commit/fba3cc3e91fe4d3c0d2092ba974ced40e8faac73))

## [0.4.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.3.0...inspequte-v0.4.0) (2026-01-18)


### Features

* split analysis load timings ([7383a12](https://github.com/KengoTODA/inspequte/commit/7383a126339176fbb6ff23d0da82ee30e71f1a51))
* split analysis timing metrics ([3cd3f16](https://github.com/KengoTODA/inspequte/commit/3cd3f16a50187fdfd12ec2615a670710781ebc64))


### Bug Fixes

* reduce callgraph edge overhead ([ddd6e90](https://github.com/KengoTODA/inspequte/commit/ddd6e907b85ad8025753e2141f68b7c26bf7888d))

## [0.3.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.2.1...inspequte-v0.3.0) (2026-01-17)


### Features

* **cli:** add baseline command ([1a4753c](https://github.com/KengoTODA/inspequte/commit/1a4753cca2ca5116b3a7d652fe42fdaa8726c4ed))
* detect array equality pitfalls ([de7b6e7](https://github.com/KengoTODA/inspequte/commit/de7b6e76038ac13bceda6193c4d42e80e7caa1e8))
* flag record components with array types ([0dba179](https://github.com/KengoTODA/inspequte/commit/0dba179d9f266f71a29c9b3e1514984aa8caefc6))

## [0.2.1](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.2.0...inspequte-v0.2.1) (2026-01-17)


### Bug Fixes

* add description to Cargo.toml to publish to crates.io ([f256109](https://github.com/KengoTODA/inspequte/commit/f2561091d9e86b5d96f1acc1d60135a98b112b74))

## [0.2.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.1.0...inspequte-v0.2.0) (2026-01-17)


### Features

* add codes for Milestone 4 partially ([cc243bc](https://github.com/KengoTODA/inspequte/commit/cc243bc722d832826fd17dff456628fb87534260))
* add SpotBugs benchmark script ([442ee45](https://github.com/KengoTODA/inspequte/commit/442ee45b7420a9e00d90697c5087270f9787b775))
* implement features for the Milestone 4 ([dd5226b](https://github.com/KengoTODA/inspequte/commit/dd5226bd54b036201928da927753947f9700f19f))
* implement Milestone 3 ([5037afb](https://github.com/KengoTODA/inspequte/commit/5037afbf7c18cbb694c2c0b1e86f9a0c45648b17))
* make some progress on the Milestone 5 ([a8daf8d](https://github.com/KengoTODA/inspequte/commit/a8daf8dbbfde110cd2a3d2dfe817422c2798d156))
