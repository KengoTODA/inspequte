# Changelog

## [0.16.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.15.1...inspequte-v0.16.0) (2026-02-11)


### Features

* **docs:** add MkDocs rule docs generation and Pages workflow ([2e80e1b](https://github.com/KengoTODA/inspequte/commit/2e80e1b693bc1ef95679fe752fb73043402c7bc1))
* **rules:** add lock_not_released_on_exception_path rule ([5b8bb59](https://github.com/KengoTODA/inspequte/commit/5b8bb593606c9479d6a646619a1edb9ec6e5035f))

## [0.15.1](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.15.0...inspequte-v0.15.1) (2026-02-10)


### Bug Fixes

* **ci:** release failed on linux-arm archtecture ([86479d8](https://github.com/KengoTODA/inspequte/commit/86479d8b18d484e7fd153628003fb017ea4d72da))
* **gradle-plugin:** publish plugin with release version ([c421040](https://github.com/KengoTODA/inspequte/commit/c421040a3529de7d032b58c9be4573c901a770be))

## [0.15.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.14.0...inspequte-v0.15.0) (2026-02-10)


### Features

* **gradle-plugin:** add optional otel collector setting ([f98efd2](https://github.com/KengoTODA/inspequte/commit/f98efd253e6b8d263f8920de2bdfa785db08d7c8))


### Bug Fixes

* add intel macOS release binary build ([6f13e0a](https://github.com/KengoTODA/inspequte/commit/6f13e0a03283711d1fd18192b9ad7b1143b99f65))
* add linux arm64 release binary build ([8cf97bc](https://github.com/KengoTODA/inspequte/commit/8cf97bc966c21fe78f52c93e2308706836a80c0d))
* **rules:** avoid slf4j false positive for kotlin reified logger ([616c75d](https://github.com/KengoTODA/inspequte/commit/616c75dfe7e2bb011dc7bfdbf491577e93caf088))
* **rules:** ignore kotlin enum when mapping in empty catch ([3575e6a](https://github.com/KengoTODA/inspequte/commit/3575e6a24ae5ef9b66c8866f54bef77bc969cbf6))

## [0.14.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.13.1...inspequte-v0.14.0) (2026-02-09)


### Features

* **gradle-plugin:** add plugin project and release automation ([3edcba5](https://github.com/KengoTODA/inspequte/commit/3edcba581b2f6a1ca266b4a46ee3bd930cfa84a6))
* **nullness:** propagate type-use through generic call flow ([dbe1e60](https://github.com/KengoTODA/inspequte/commit/dbe1e60fc751bfda25a76413ebc02be5492226eb))

## [0.13.1](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.13.0...inspequte-v0.13.1) (2026-02-08)


### Bug Fixes

* **ci:** include LICENSE in release archives ([88fb0ae](https://github.com/KengoTODA/inspequte/commit/88fb0ae698246e064ee2dd753581f587ba21cb19))
* include README in release archives ([47de8eb](https://github.com/KengoTODA/inspequte/commit/47de8eb6afae7966d44e51f809275ac4a74049a7))
* **release:** checkout Git repo before publish GitHub Release ([d884758](https://github.com/KengoTODA/inspequte/commit/d884758206787826d065dca09f376857e08efb9a))

## [0.13.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.12.0...inspequte-v0.13.0) (2026-02-08)

### Bug Fixes

* broken release-please at v0.11.0 release ([ae80c73](https://github.com/KengoTODA/inspequte/commit/ae80c73dbcfd6af425fcf37dd9bd39b689273fc7))

## [0.12.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.11.0...inspequte-v0.12.0) (2026-02-08)

### Features

* attach pre-built binary to GitHub Releases

## [0.11.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.10.2...inspequte-v0.11.0) (2026-02-08)


### Features

* add type-use nullness parsing and override checks ([#29](https://github.com/KengoTODA/inspequte/issues/29)) ([67a4f29](https://github.com/KengoTODA/inspequte/commit/67a4f2968794bcfc49adae1dc089c1f6ddb2c921))

## [0.10.2](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.10.1...inspequte-v0.10.2) (2026-01-31)


### Bug Fixes

* skip classpath classes in insecure api ([067719e](https://github.com/KengoTODA/inspequte/commit/067719ee885f9c908f5750a5758b4b6d6c32a5d5))

## [0.10.1](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.10.0...inspequte-v0.10.1) (2026-01-31)


### Bug Fixes

* add physical locations for class results ([52bfd95](https://github.com/KengoTODA/inspequte/commit/52bfd9564cf7c6519418326c98e197e89ba4c942))

## [0.10.0](https://github.com/KengoTODA/inspequte/compare/inspequte-v0.9.0...inspequte-v0.10.0) (2026-01-31)


### Features

* add log4j2 format const rule ([e32d352](https://github.com/KengoTODA/inspequte/commit/e32d352262f1ca1adaa9e505b3725f03a84da84f))
* add log4j2 illegal passed class rule ([d323df1](https://github.com/KengoTODA/inspequte/commit/d323df144483bdf5cd28ecc13999e377c6f53885))
* add log4j2 logger should be final rule ([9f1b969](https://github.com/KengoTODA/inspequte/commit/9f1b9697340eed8c8d82a727d0a27a76956ad824))
* add log4j2 logger should be private rule ([c1c9359](https://github.com/KengoTODA/inspequte/commit/c1c93597d02621941a21912bc807889885822e51))
* add log4j2 manually provided message rule ([c2d9189](https://github.com/KengoTODA/inspequte/commit/c2d918912f5a1cb9f198e0070cf6dd997820c148))
* add log4j2 sign only format rule ([a1df00a](https://github.com/KengoTODA/inspequte/commit/a1df00a92527c6d1e8e4dab7f3d8f42f6221b95a))
* add log4j2 unknown array rule ([8fe13d0](https://github.com/KengoTODA/inspequte/commit/8fe13d0c139c430f7ae11cb553cb8d9a2ff836d2))
* implement automatic rule registration using inventory crate ([e4ed66c](https://github.com/KengoTODA/inspequte/commit/e4ed66cd4e01027bdc924e78dbb462e6080f713c))


### Bug Fixes

* avoid jar uris in sarif locations ([a11a77a](https://github.com/KengoTODA/inspequte/commit/a11a77a22e45a010af6675e81aa4370e856142b0))
* ignore missing input directories ([ecd40cd](https://github.com/KengoTODA/inspequte/commit/ecd40cdd6741572754d01742f217e66eb1735a6c))
* **sarif:** add semanticVersion to sarif output ([f60d347](https://github.com/KengoTODA/inspequte/commit/f60d3474170ac847ffd69712f3c3adc89dab7278))


### Performance Improvements

* skip logger rules when framework absent ([d32a2f2](https://github.com/KengoTODA/inspequte/commit/d32a2f271f7006cc5095012ad704bf113647ace1))

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
