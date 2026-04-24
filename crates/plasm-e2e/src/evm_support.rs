use alloy::{
    hex,
    network::EthereumWallet,
    primitives::{address, Address, FixedBytes, U256},
    providers::{Provider, ProviderBuilder, RootProvider},
    signers::local::PrivateKeySigner,
    sol,
    transports::http::Http,
};
use alloy_rpc_client::RpcClient;
use plasm_runtime::{ExecutionConfig, ExecutionEngine};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tempfile::TempDir;
use testcontainers_modules::{
    anvil::AnvilNode,
    testcontainers::{runners::AsyncRunner, ContainerAsync},
};

#[allow(dead_code)]
pub const ANVIL_ADDRESS_0: Address = address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
pub const ANVIL_ADDRESS_1: Address = address!("70997970C51812dc3A010C7d01b50e0d17dc79C8");
pub const ANVIL_ADDRESS_2: Address = address!("3C44CdDdB6a900fa2b585dd299e03d12FA4293BC");

static CONTAINER_ENV_INIT: OnceLock<()> = OnceLock::new();

sol! {
    #[sol(rpc, bytecode = "60c0604052600a6080908152692a32b9ba102a37b5b2b760b11b60a0525f906100289082610108565b50604080518082019091526004815263151154d560e21b60208201526001906100519082610108565b506002805460ff1916601217905534801561006a575f80fd5b506101c2565b634e487b7160e01b5f52604160045260245ffd5b600181811c9082168061009857607f821691505b6020821081036100b657634e487b7160e01b5f52602260045260245ffd5b50919050565b601f82111561010357805f5260205f20601f840160051c810160208510156100e15750805b601f840160051c820191505b81811015610100575f81556001016100ed565b50505b505050565b81516001600160401b0381111561012157610121610070565b6101358161012f8454610084565b846100bc565b6020601f821160018114610167575f83156101505750848201515b5f19600385901b1c1916600184901b178455610100565b5f84815260208120601f198516915b828110156101965787850151825560209485019460019092019101610176565b50848210156101b357868401515f19600387901b60f8161c191681555b50505050600190811b01905550565b610770806101cf5f395ff3fe608060405234801561000f575f80fd5b506004361061009b575f3560e01c806340c10f191161006357806340c10f191461012957806370a082311461013e57806395d89b411461015d578063a9059cbb14610165578063dd62ed3e14610178575f80fd5b806306fdde031461009f578063095ea7b3146100bd57806318160ddd146100e057806323b872dd146100f7578063313ce5671461010a575b5f80fd5b6100a76101a2565b6040516100b491906105c5565b60405180910390f35b6100d06100cb366004610615565b61022d565b60405190151581526020016100b4565b6100e960035481565b6040519081526020016100b4565b6100d061010536600461063d565b610299565b6002546101179060ff1681565b60405160ff90911681526020016100b4565b61013c610137366004610615565b61044f565b005b6100e961014c366004610677565b60046020525f908152604090205481565b6100a76104d7565b6100d0610173366004610615565b6104e4565b6100e9610186366004610697565b600560209081525f928352604080842090915290825290205481565b5f80546101ae906106c8565b80601f01602080910402602001604051908101604052809291908181526020018280546101da906106c8565b80156102255780601f106101fc57610100808354040283529160200191610225565b820191905f5260205f20905b81548152906001019060200180831161020857829003601f168201915b505050505081565b335f8181526005602090815260408083206001600160a01b038716808552925280832085905551919290917f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925906102879086815260200190565b60405180910390a35060015b92915050565b6001600160a01b0383165f9081526005602090815260408083203384529091528120548211156103095760405162461bcd60e51b8152602060048201526016602482015275496e73756666696369656e7420616c6c6f77616e636560501b60448201526064015b60405180910390fd5b6001600160a01b0384165f908152600460205260409020548211156103675760405162461bcd60e51b8152602060048201526014602482015273496e73756666696369656e742062616c616e636560601b6044820152606401610300565b6001600160a01b0384165f90815260056020908152604080832033845290915281208054849290610399908490610714565b90915550506001600160a01b0384165f90815260046020526040812080548492906103c5908490610714565b90915550506001600160a01b0383165f90815260046020526040812080548492906103f1908490610727565b92505081905550826001600160a01b0316846001600160a01b03167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef8460405161043d91815260200190565b60405180910390a35060019392505050565b6001600160a01b0382165f9081526004602052604081208054839290610476908490610727565b925050819055508060035f82825461048e9190610727565b90915550506040518181526001600160a01b038316905f907fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef9060200160405180910390a35050565b600180546101ae906106c8565b335f908152600460205260408120548211156105395760405162461bcd60e51b8152602060048201526014602482015273496e73756666696369656e742062616c616e636560601b6044820152606401610300565b335f9081526004602052604081208054849290610557908490610714565b90915550506001600160a01b0383165f9081526004602052604081208054849290610583908490610727565b90915550506040518281526001600160a01b0384169033907fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef90602001610287565b602081525f82518060208401528060208501604085015e5f604082850101526040601f19601f83011684010191505092915050565b80356001600160a01b0381168114610610575f80fd5b919050565b5f8060408385031215610626575f80fd5b61062f836105fa565b946020939093013593505050565b5f805f6060848603121561064f575f80fd5b610658846105fa565b9250610666602085016105fa565b929592945050506040919091013590565b5f60208284031215610687575f80fd5b610690826105fa565b9392505050565b5f80604083850312156106a8575f80fd5b6106b1836105fa565b91506106bf602084016105fa565b90509250929050565b600181811c908216806106dc57607f821691505b6020821081036106fa57634e487b7160e01b5f52602260045260245ffd5b50919050565b634e487b7160e01b5f52601160045260245ffd5b8181038181111561029357610293610700565b808201808211156102935761029361070056fea2646970667358221220789f36a58c6b99a0418160bd543a86b7d18c6402ad07cac78d3890c24a1c326d64736f6c634300081a0033")]
    contract SimpleERC20 {
        string public name;
        string public symbol;
        uint8 public decimals;
        uint256 public totalSupply;

        mapping(address => uint256) public balanceOf;
        mapping(address => mapping(address => uint256)) public allowance;

        event Transfer(address indexed from, address indexed to, uint256 value);
        event Approval(address indexed owner, address indexed spender, uint256 value);

        function mint(address to, uint256 amount) external;
        function transfer(address to, uint256 amount) external returns (bool);
        function approve(address spender, uint256 amount) external returns (bool);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
    }
}

pub struct EvmTestContext {
    _node: ContainerAsync<AnvilNode>,
    pub endpoint: String,
}

impl EvmTestContext {
    pub async fn new() -> Self {
        init_container_runtime_env();
        let node = AnvilNode::latest().start().await.unwrap();
        let endpoint = format!(
            "http://localhost:{}",
            node.get_host_port_ipv4(8545).await.unwrap()
        );
        Self {
            _node: node,
            endpoint,
        }
    }

    pub fn read_provider(&self) -> impl Provider + Clone {
        ProviderBuilder::new().connect_http(self.endpoint.parse().unwrap())
    }

    pub fn signer_provider(&self, account_index: u32) -> impl Provider + Clone {
        let http = Http::new(self.endpoint.parse().unwrap());
        let base_provider = RpcClient::new(http, false);
        let root_provider = RootProvider::new(base_provider);
        let wallet = EthereumWallet::from(derive_anvil_signer(account_index));
        ProviderBuilder::new()
            .wallet(wallet)
            .connect_provider(root_provider)
    }

    pub async fn deploy_test_erc20(&self) -> Address {
        let provider = self.signer_provider(0);
        let contract = SimpleERC20::deploy(&provider).await.unwrap();
        *contract.address()
    }

    pub async fn mint(&self, token: Address, to: Address, amount: U256) {
        let provider = self.signer_provider(0);
        let contract = SimpleERC20::new(token, &provider);
        contract
            .mint(to, amount)
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();
    }

    pub async fn latest_block(&self) -> u64 {
        self.read_provider().get_block_number().await.unwrap()
    }

    pub fn write_schema_dir(&self, token: Address) -> TempDir {
        let dir = TempDir::new().unwrap();
        let domain_src = find_domain_fixture_path();
        let domain_contents = fs::read_to_string(domain_src).unwrap();
        fs::write(dir.path().join("domain.yaml"), domain_contents).unwrap();
        fs::write(
            dir.path().join("mappings.yaml"),
            render_mappings_yaml(token),
        )
        .unwrap();
        dir
    }
}

pub fn make_engine(endpoint: &str) -> ExecutionEngine {
    let config = ExecutionConfig {
        base_url: Some(endpoint.to_string()),
        ..Default::default()
    };
    ExecutionEngine::new(config).unwrap()
}

fn derive_anvil_signer(index: u32) -> PrivateKeySigner {
    let key_hex = match index {
        0 => {
            let parts = [
                "ac0974bec39a17e36ba4a6b4d238ff94",
                "4bacb478cbed5efcae784d7bf4f2ff80",
            ];
            format!("{first}{second}", first = parts[0], second = parts[1])
        }
        1 => {
            let parts = [
                "59c6995e998f97a5a0044966f0945389",
                "dc9e86dae88c7a8412f4603b6b78690d",
            ];
            format!("{first}{second}", first = parts[0], second = parts[1])
        }
        2 => {
            let parts = [
                "5de4111afa1a4b94908f83103eb1f170",
                "6367c2e68ca870fc3fb9a804cdab365a",
            ];
            format!("{first}{second}", first = parts[0], second = parts[1])
        }
        _ => panic!("unsupported anvil account index {index}"),
    };

    let key_bytes = hex::decode(&key_hex).unwrap();
    let fixed_bytes: FixedBytes<32> = FixedBytes::from_slice(&key_bytes);
    PrivateKeySigner::from_bytes(&fixed_bytes).unwrap()
}

fn render_mappings_yaml(token: Address) -> String {
    format!(
        r#"balance_get:
  transport: evm_call
  chain: 31337
  contract:
    type: const
    value: "{token}"
  function: "function balanceOf(address account) view returns (uint256 balance)"
  args:
    - type: var
      name: id
  decode:
    account:
      type: input
      index: 0
    balance:
      type: output
      index: 0
transfer_query:
  transport: evm_logs
  chain: 31337
  contract:
    type: const
    value: "{token}"
  event: "event Transfer(address indexed from, address indexed to, uint256 value)"
  pagination:
    location: block_range
    params:
      range_size:
        fixed: 1
  decode:
    event_id:
      type: log_meta
      key: event_id
    tx_hash:
      type: log_meta
      key: transaction_hash
    log_index:
      type: log_meta
      key: log_index
    from:
      type: topic
      index: 0
    to:
      type: topic
      index: 1
    value:
      type: data
      index: 0
"#
    )
}

fn find_domain_fixture_path() -> PathBuf {
    fixture_candidates()
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("apis/evm-erc20/domain.yaml not found"))
}

fn fixture_candidates() -> [PathBuf; 2] {
    [
        Path::new("apis/evm-erc20/domain.yaml").to_path_buf(),
        Path::new("../../apis/evm-erc20/domain.yaml").to_path_buf(),
    ]
}

fn init_container_runtime_env() {
    CONTAINER_ENV_INIT.get_or_init(|| {
        if env::var_os("DOCKER_HOST").is_some() || Path::new("/var/run/docker.sock").exists() {
            return;
        }

        let podman_sock = Path::new(&home_dir_string())
            .join(".local/share/containers/podman/machine/podman.sock");
        if podman_sock.exists() {
            // SAFETY: called once during lazy static init, before any concurrent access.
            unsafe {
                env::set_var(
                    "DOCKER_HOST",
                    format!("unix://{path}", path = podman_sock.display()),
                );
            }
        }
    });
}

fn home_dir_string() -> String {
    env::var("HOME").unwrap_or_else(|_| String::from("/tmp"))
}
