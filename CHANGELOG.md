# Changelog

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
