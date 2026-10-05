// Read-only identity bridge. No BindToObject, waveInOpen, Activate or PCM API.
#include <windows.h>
#include <dshow.h>
#include <mmddk.h>
#include <mmdeviceapi.h>
#include <wrl/client.h>
#include <string>
#include <vector>
#include <iostream>
#include <algorithm>
using Microsoft::WRL::ComPtr;
static std::string quoted(const std::wstring& s) {
    int n=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),nullptr,0,nullptr,nullptr);
    if(n<0 || (n==0 && !s.empty())) throw 1;
    std::string u(n,'\0'); if(n)WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),u.data(),n,nullptr,nullptr);
    std::string q="\"";
    for(unsigned char c:u){if(c=='"'||c=='\\'){q+='\\';q+=(char)c;}else if(c<32)throw 2;else q+=(char)c;}
    return q+'"';
}
int main(){
    HRESULT hr=CoInitializeEx(nullptr,COINIT_MULTITHREADED);if(FAILED(hr))return 1;
    int code=0;
    try{
        ComPtr<ICreateDevEnum> devices;ComPtr<IEnumMoniker> enumerator;ComPtr<IMMDeviceEnumerator> mm;
        hr=CoCreateInstance(CLSID_SystemDeviceEnum,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&devices));if(FAILED(hr))throw 3;
        hr=devices->CreateClassEnumerator(CLSID_AudioInputDeviceCategory,&enumerator,0);if(hr!=S_OK)throw 4;
        hr=CoCreateInstance(__uuidof(MMDeviceEnumerator),nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&mm));if(FAILED(hr))throw 5;
        bool first=true;std::cout<<"[";
        for(unsigned count=0;count<64;++count){
            ComPtr<IMoniker> moniker;ULONG got=0;hr=enumerator->Next(1,&moniker,&got);if(hr==S_FALSE)break;if(hr!=S_OK||got!=1)throw 6;
            ComPtr<IPropertyBag> bag;ComPtr<IBindCtx> context;
            if(FAILED(CreateBindCtx(0,&context)))throw 7;
            LPOLESTR text=nullptr;hr=moniker->GetDisplayName(context.Get(),nullptr,&text);if(FAILED(hr)||!text)throw 8;
            std::wstring alternative=text;CoTaskMemFree(text);if(alternative.size()>4096)throw 9;
            std::replace(alternative.begin(),alternative.end(),L':',L'_');
            if(!first)std::cout<<",";first=false;
            std::cout<<"{\"alternative_name\":"<<quoted(alternative);
            hr=moniker->BindToStorage(nullptr,nullptr,IID_PPV_ARGS(&bag));
            VARIANT value;VariantInit(&value);long id=-1;
            if(SUCCEEDED(hr)){hr=bag->Read(L"WaveInID",&value,nullptr);if(SUCCEEDED(hr)&&value.vt==VT_I4)id=value.lVal;}
            VariantClear(&value);
            if(id<0 || (UINT)id>=waveInGetNumDevs()){std::cout<<",\"status\":\"NO_VALID_WAVEIN_ID\"}";continue;}
            SIZE_T size=0;MMRESULT result=waveInMessage((HWAVEIN)(UINT_PTR)id,DRV_QUERYFUNCTIONINSTANCEIDSIZE,(DWORD_PTR)&size,0);
            if(result!=0||size<2||size>4096||size%sizeof(wchar_t)){std::cout<<",\"status\":\"ID_SIZE_REJECTED\",\"mmresult\":"<<result<<"}";continue;}
            std::vector<wchar_t> buffer(size/sizeof(wchar_t),L'\0');
            result=waveInMessage((HWAVEIN)(UINT_PTR)id,DRV_QUERYFUNCTIONINSTANCEID,(DWORD_PTR)buffer.data(),size);
            if(result!=0||buffer.back()!=L'\0'||buffer[0]==L'\0'){std::cout<<",\"status\":\"ID_QUERY_REJECTED\",\"mmresult\":"<<result<<"}";continue;}
            ComPtr<IMMDevice> endpoint;ComPtr<IMMEndpoint> flow;DWORD state=0;EDataFlow direction=eAll;LPWSTR uid=nullptr;
            hr=mm->GetDevice(buffer.data(),&endpoint);
            if(SUCCEEDED(hr))hr=endpoint.As(&flow);
            if(SUCCEEDED(hr))hr=flow->GetDataFlow(&direction);
            if(SUCCEEDED(hr))hr=endpoint->GetState(&state);
            if(SUCCEEDED(hr))hr=endpoint->GetId(&uid);
            if(FAILED(hr)||!uid){std::cout<<",\"status\":\"ENDPOINT_QUERY_REJECTED\",\"hresult\":"<<hr<<"}";continue;}
            std::wstring endpoint_id=uid;CoTaskMemFree(uid);
            std::cout<<",\"status\":\"MAPPED_METADATA_ONLY\",\"wavein_id\":"<<id<<",\"function_instance_id\":"<<quoted(buffer.data())<<",\"endpoint_uid\":"<<quoted(endpoint_id)<<",\"flow\":"<<direction<<",\"state_bits\":"<<state<<"}";
            if(count==63)throw 10;
        }
        std::cout<<"]\n";
    }catch(...){code=1;std::cerr<<"read-only identity bridge failed\n";}
    CoUninitialize();return code;
}
