docker build -t image-name -f ./Dockerfile .
docker create --name extract-container image-name
mkdir -p output 
docker cp extract-container:/build/target/x86_64-unknown-linux-musl/release/jonah ./output/jonah-linux-x86_64
docker rm extract-container

