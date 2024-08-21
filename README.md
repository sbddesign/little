# little

Little is a little lightning node. The goal, beyond just being a personal learning experiment, is to create a very opinionated lightning node that is designed for simply sending and receiving bitcoin. It exposes enough channel management to the user to get the job done, but assumes the existence of an LSP. Also uses minimal system resources.

## Devlopment

Run `cargo build` for dev or `cargo build --release` for prod.

## Roadmap

- [ ] Create `getinfo`
- [ ] Create `makenode`
- [ ] Create `getaddress`
