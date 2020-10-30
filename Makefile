build:
	cargo build

build-linux:
	ssh rediger "mkdir -p code/ferris-bot"
	rsync -a src Cargo* index.html rediger:code/ferris-bot
	ssh rediger 'cd code/ferris-bot && PATH=/home/jer/.cargo/bin:$$PATH cargo build --release'
	rsync -a rediger:code/ferris-bot/target/release/ferris-bot .

deploy: build-linux
	rsync -a ferris-bot waasabi:apps/ferris-bot/ferris-bot
	ssh waasabi 'PATH=/home/rustfest/.nvm/versions/node/v12.18.4/bin:$$PATH pm2 restart ferris-bot'
