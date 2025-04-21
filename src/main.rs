use futures::executor::block_on;
use librustycubes::run;

fn main() {
    block_on(run())
}
